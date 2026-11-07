use std::fmt;
use std::path::Path;

use chrono::{Local, NaiveDate, NaiveDateTime};
use rusqlite::{Connection, Row, params};

use super::migrations::migrate;
use crate::domain::category::Category;
use crate::domain::event::{Event, Occurrence, occurrences_in};
use crate::domain::recurrence::RecurrenceRule;

const DT_FMT: &str = "%Y-%m-%dT%H:%M:%S";

#[derive(Debug)]
pub enum StorageError {
    Sql(rusqlite::Error),
    Io(std::io::Error),
    Corrupt(String),
    NotFound,
    Migration(String),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sql(e) => write!(f, "Erro no banco de dados: {e}"),
            Self::Io(e) => write!(f, "Erro de arquivo: {e}"),
            Self::Corrupt(msg) => write!(f, "Dados corrompidos: {msg}"),
            Self::NotFound => write!(f, "Evento não encontrado"),
            Self::Migration(msg) => write!(f, "Falha ao atualizar o banco: {msg}"),
        }
    }
}

impl std::error::Error for StorageError {}

impl From<rusqlite::Error> for StorageError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sql(e)
    }
}

impl From<std::io::Error> for StorageError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Raw column values, converted to domain types outside the row callback.
struct RawEvent {
    id: i64,
    uid: String,
    category_id: i64,
    title: String,
    description: String,
    location: String,
    all_day: bool,
    start: String,
    end: String,
    rrule: Option<String>,
    reminder: Option<i64>,
}

impl RawEvent {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            uid: row.get(1)?,
            category_id: row.get(2)?,
            title: row.get(3)?,
            description: row.get(4)?,
            location: row.get(5)?,
            all_day: row.get::<_, i64>(6)? != 0,
            start: row.get(7)?,
            end: row.get(8)?,
            rrule: row.get(9)?,
            reminder: row.get(10)?,
        })
    }

    fn into_event(self) -> Result<Event, StorageError> {
        let corrupt = |what: &str| StorageError::Corrupt(format!("evento {}: {what}", self.id));
        let start = NaiveDateTime::parse_from_str(&self.start, DT_FMT).map_err(|_| corrupt("início"))?;
        let end = NaiveDateTime::parse_from_str(&self.end, DT_FMT).map_err(|_| corrupt("fim"))?;
        let rrule = match self.rrule {
            Some(text) => Some(RecurrenceRule::parse(&text).map_err(|_| corrupt("recorrência"))?),
            None => None,
        };
        Ok(Event {
            id: Some(self.id),
            uid: self.uid,
            title: self.title,
            description: self.description,
            location: self.location,
            category_id: self.category_id,
            all_day: self.all_day,
            start,
            end,
            rrule,
            reminder_minutes: self.reminder,
        })
    }
}

const EVENT_COLUMNS: &str = "id, uid, category_id, title, description, location, all_day, \
     start_local, end_local, rrule, reminder_minutes";

pub struct Repository {
    conn: Connection,
}

impl Repository {
    /// Opens (or creates) the database file, creating its parent directory if needed.
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Self::from_connection(Connection::open(path)?)
    }

    #[cfg(test)]
    pub fn in_memory() -> Result<Self, StorageError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(mut conn: Connection) -> Result<Self, StorageError> {
        conn.pragma_update(None, "foreign_keys", true)?;
        migrate(&mut conn).map_err(|e| StorageError::Migration(e.to_string()))?;
        Ok(Self { conn })
    }

    fn now() -> String {
        Local::now().naive_local().format(DT_FMT).to_string()
    }

    /// Reads one value from the `settings` table.
    pub fn get_setting(&self, key: &str) -> Result<Option<String>, StorageError> {
        match self.conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| row.get(0)) {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn categories(&self) -> Result<Vec<Category>, StorageError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, accent, visible FROM categories ORDER BY id")?;
        let rows = stmt.query_map([], |row| {
            Ok(Category {
                id: row.get(0)?,
                name: row.get(1)?,
                accent: row.get(2)?,
                visible: row.get::<_, i64>(3)? != 0,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn set_category_visible(&self, id: i64, visible: bool) -> Result<(), StorageError> {
        self.conn
            .execute("UPDATE categories SET visible = ?1 WHERE id = ?2", params![visible, id])?;
        Ok(())
    }

    /// Returns the id of the category with this name, creating it (with the next accent) if needed.
    pub fn ensure_category(&self, name: &str) -> Result<i64, StorageError> {
        let name = if name.trim().is_empty() { "Geral" } else { name.trim() };
        let accent: i64 = self.conn.query_row("SELECT COUNT(*) FROM categories", [], |r| r.get(0))?;
        self.conn.execute(
            "INSERT OR IGNORE INTO categories (name, accent) VALUES (?1, ?2)",
            params![name, accent % 8],
        )?;
        Ok(self.conn.query_row("SELECT id FROM categories WHERE name = ?1", [name], |r| r.get(0))?)
    }

    pub fn insert_event(&self, event: &Event) -> Result<i64, StorageError> {
        let now = Self::now();
        self.conn.execute(
            "INSERT INTO events (uid, category_id, title, description, location, all_day, \
             start_local, end_local, rrule, reminder_minutes, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)",
            params![
                event.uid,
                event.category_id,
                event.title,
                event.description,
                event.location,
                event.all_day,
                event.start.format(DT_FMT).to_string(),
                event.end.format(DT_FMT).to_string(),
                event.rrule.as_ref().map(RecurrenceRule::to_rrule_string),
                event.reminder_minutes,
                now,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_event(&self, event: &Event) -> Result<(), StorageError> {
        let id = event.id.ok_or(StorageError::NotFound)?;
        let changed = self.conn.execute(
            "UPDATE events SET category_id = ?2, title = ?3, description = ?4, location = ?5, \
             all_day = ?6, start_local = ?7, end_local = ?8, rrule = ?9, reminder_minutes = ?10, \
             updated_at = ?11 WHERE id = ?1",
            params![
                id,
                event.category_id,
                event.title,
                event.description,
                event.location,
                event.all_day,
                event.start.format(DT_FMT).to_string(),
                event.end.format(DT_FMT).to_string(),
                event.rrule.as_ref().map(RecurrenceRule::to_rrule_string),
                event.reminder_minutes,
                Self::now(),
            ],
        )?;
        if changed == 0 {
            return Err(StorageError::NotFound);
        }
        Ok(())
    }

    pub fn delete_event(&self, id: i64) -> Result<(), StorageError> {
        self.conn.execute("DELETE FROM events WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn event(&self, id: i64) -> Result<Option<Event>, StorageError> {
        let raw = self
            .conn
            .query_row(
                &format!("SELECT {EVENT_COLUMNS} FROM events WHERE id = ?1"),
                [id],
                RawEvent::from_row,
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        raw.map(RawEvent::into_event).transpose()
    }

    pub fn events(&self) -> Result<Vec<Event>, StorageError> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {EVENT_COLUMNS} FROM events ORDER BY start_local"))?;
        let raws = stmt
            .query_map([], RawEvent::from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        raws.into_iter().map(RawEvent::into_event).collect()
    }

    /// Cancels one occurrence of a series (iCalendar EXDATE).
    pub fn cancel_occurrence(&self, event_id: i64, original_start: NaiveDateTime) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT OR IGNORE INTO event_exceptions (event_id, original_start, kind) VALUES (?1, ?2, 'cancelled')",
            params![event_id, original_start.format(DT_FMT).to_string()],
        )?;
        Ok(())
    }

    pub fn cancellations(&self, event_id: i64) -> Result<Vec<NaiveDateTime>, StorageError> {
        let mut stmt = self
            .conn
            .prepare("SELECT original_start FROM event_exceptions WHERE event_id = ?1")?;
        let rows = stmt.query_map([event_id], |row| row.get::<_, String>(0))?;
        rows.map(|r| {
            let text = r?;
            NaiveDateTime::parse_from_str(&text, DT_FMT)
                .map_err(|_| StorageError::Corrupt(format!("exceção do evento {event_id}")))
        })
        .collect()
    }

    /// All occurrences overlapping the inclusive date range `[from, to]`, across every category.
    pub fn occurrences(&self, from: NaiveDate, to: NaiveDate) -> Result<Vec<Occurrence>, StorageError> {
        let mut out = Vec::new();
        for event in self.events()? {
            let id = event.id.ok_or(StorageError::NotFound)?;
            let cancelled = self.cancellations(id)?;
            out.extend(occurrences_in(&event, &cancelled, from, to));
        }
        out.sort_by_key(|o| (o.start, o.event.title.clone()));
        Ok(out)
    }

    /// Records that a reminder fired for this occurrence. Returns `false` if it already fired.
    pub fn mark_reminder(&self, event_id: i64, occurrence: NaiveDateTime) -> Result<bool, StorageError> {
        let changed = self.conn.execute(
            "INSERT OR IGNORE INTO reminders_log (event_id, occurrence_start) VALUES (?1, ?2)",
            params![event_id, occurrence.format(DT_FMT).to_string()],
        )?;
        Ok(changed > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::recurrence::Freq;
    use chrono::NaiveDate;

    fn at(y: i32, m: u32, d: u32, h: u32, min: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(h, min, 0).unwrap()
    }

    fn sample(category_id: i64, rrule: Option<RecurrenceRule>) -> Event {
        Event {
            id: None,
            uid: crate::domain::event::new_uid(),
            title: "Consulta".into(),
            description: "Levar exames".into(),
            location: "Clínica".into(),
            category_id,
            all_day: false,
            start: at(2026, 4, 1, 14, 0),
            end: at(2026, 4, 1, 15, 0),
            rrule,
            reminder_minutes: Some(30),
        }
    }

    #[test]
    fn migrations_seed_default_categories() {
        let repo = Repository::in_memory().unwrap();
        let names: Vec<String> = repo.categories().unwrap().into_iter().map(|c| c.name).collect();
        assert_eq!(names, vec!["Trabalho", "Pessoal", "Saúde"]);
    }

    #[test]
    fn crud_round_trip() {
        let repo = Repository::in_memory().unwrap();
        let cat = repo.categories().unwrap()[0].id;
        let id = repo.insert_event(&sample(cat, None)).unwrap();

        let mut stored = repo.event(id).unwrap().unwrap();
        assert_eq!(stored.title, "Consulta");
        assert_eq!(stored.reminder_minutes, Some(30));

        stored.title = "Consulta médica".into();
        repo.update_event(&stored).unwrap();
        assert_eq!(repo.event(id).unwrap().unwrap().title, "Consulta médica");

        repo.delete_event(id).unwrap();
        assert!(repo.event(id).unwrap().is_none());
    }

    #[test]
    fn recurrence_and_cancellations_persist() {
        let repo = Repository::in_memory().unwrap();
        let cat = repo.categories().unwrap()[0].id;
        let rule = RecurrenceRule { count: Some(3), ..RecurrenceRule::simple(Freq::Daily) };
        let id = repo.insert_event(&sample(cat, Some(rule.clone()))).unwrap();
        repo.cancel_occurrence(id, at(2026, 4, 2, 14, 0)).unwrap();

        let stored = repo.event(id).unwrap().unwrap();
        assert_eq!(stored.rrule, Some(rule));

        let from = NaiveDate::from_ymd_opt(2026, 4, 1).unwrap();
        let to = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();
        assert_eq!(repo.occurrences(from, to).unwrap().len(), 2);
    }

    #[test]
    fn reminder_fires_once() {
        let repo = Repository::in_memory().unwrap();
        let cat = repo.categories().unwrap()[0].id;
        let id = repo.insert_event(&sample(cat, None)).unwrap();
        assert!(repo.mark_reminder(id, at(2026, 4, 1, 14, 0)).unwrap());
        assert!(!repo.mark_reminder(id, at(2026, 4, 1, 14, 0)).unwrap());
    }

    #[test]
    fn deleting_event_removes_its_exceptions() {
        let repo = Repository::in_memory().unwrap();
        let cat = repo.categories().unwrap()[0].id;
        let rule = RecurrenceRule::simple(Freq::Daily);
        let id = repo.insert_event(&sample(cat, Some(rule))).unwrap();
        repo.cancel_occurrence(id, at(2026, 4, 2, 14, 0)).unwrap();
        repo.delete_event(id).unwrap();
        assert!(repo.cancellations(id).unwrap().is_empty());
    }

    #[test]
    fn updating_missing_event_is_an_error_not_a_panic() {
        let repo = Repository::in_memory().unwrap();
        let mut event = sample(1, None);
        event.id = Some(999);
        assert!(matches!(repo.update_event(&event), Err(StorageError::NotFound)));
    }

    #[test]
    fn migrations_are_idempotent_on_reopen() {
        let dir = std::env::temp_dir().join(format!("oxide-cal-test-{}", crate::domain::event::new_uid()));
        let path = dir.join("calendar.db");
        {
            let repo = Repository::open(&path).unwrap();
            repo.ensure_category("Projetos").unwrap();
        }
        let repo = Repository::open(&path).unwrap();
        assert_eq!(repo.categories().unwrap().len(), 4);
        std::fs::remove_dir_all(dir).ok();
    }
}
