//! UI-agnostic state of the calendar: navigation, the data each view needs, and the event editor.
//! The Slint bridge only forwards user intents here and renders the snapshots it returns.

use std::collections::{HashMap, HashSet};

use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, Timelike};

use crate::domain::category::Category;
use crate::domain::datetime::{clamped_date, month_grid, week_start};
use crate::domain::event::{Event, Occurrence, new_uid};
use crate::domain::layout::layout;
use crate::domain::recurrence::{Freq, RecurrenceRule};
use crate::domain::search;
use crate::ics::{self, IcsEvent};
use crate::storage::Repository;

pub const MONTH_NAMES: [&str; 12] = [
    "janeiro", "fevereiro", "março", "abril", "maio", "junho", "julho", "agosto", "setembro",
    "outubro", "novembro", "dezembro",
];
/// Indexed by `num_days_from_sunday()`.
pub const WEEKDAY_NAMES: [&str; 7] = ["domingo", "segunda", "terça", "quarta", "quinta", "sexta", "sábado"];
const WEEKDAY_SHORT: [&str; 7] = ["dom", "seg", "ter", "qua", "qui", "sex", "sáb"];

/// Options of the editor's recurrence selector. Index 5 keeps a custom rule untouched.
pub const RECURRENCE_KEEP: i32 = 5;
/// Options of the editor's reminder selector. Index 7 keeps a custom reminder untouched.
pub const REMINDER_KEEP: i32 = 7;

const MONTH_VISIBLE_ITEMS: usize = 3;
const MIN_BLOCK_MINUTES: i64 = 30;
pub const AGENDA_DAYS: i64 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Month,
    Week,
    Day,
    Agenda,
}

impl View {
    pub fn from_index(index: i32) -> Self {
        match index {
            0 => Self::Month,
            1 => Self::Week,
            2 => Self::Day,
            _ => Self::Agenda,
        }
    }
}

pub struct MonthCell {
    pub date: NaiveDate,
    pub label: String,
    pub in_month: bool,
    pub today: bool,
    /// Up to `MONTH_VISIBLE_ITEMS` labels with their category accent.
    pub items: Vec<(String, i32)>,
    pub more: usize,
}

pub struct Block {
    pub event_id: i64,
    pub start_key: String,
    pub title: String,
    pub subtitle: String,
    pub top_min: i64,
    pub height_min: i64,
    pub column: usize,
    pub columns: usize,
    pub accent: i32,
}

pub struct AllDayChip {
    pub event_id: i64,
    pub start_key: String,
    pub title: String,
    pub accent: i32,
}

pub struct DayColumn {
    pub date: NaiveDate,
    pub label: String,
    pub today: bool,
    pub all_day: Vec<AllDayChip>,
    pub blocks: Vec<Block>,
}

pub struct AgendaRow {
    pub event_id: i64,
    pub start_key: String,
    pub title: String,
    pub when: String,
    pub location: String,
    pub accent: i32,
}

pub struct Snapshot {
    pub title: String,
    pub view: View,
    pub month_cells: Vec<MonthCell>,
    pub days: Vec<DayColumn>,
    pub agenda: Vec<AgendaRow>,
    pub categories: Vec<Category>,
    pub empty_message: String,
}

/// Values for the editor dialog, as displayed.
pub struct EditorData {
    pub editing: bool,
    pub recurring: bool,
    pub title: String,
    pub description: String,
    pub location: String,
    pub category_index: i32,
    pub all_day: bool,
    pub start_date: String,
    pub start_time: String,
    pub end_date: String,
    pub end_time: String,
    pub recurrence_index: i32,
    pub reminder_index: i32,
}

/// Values submitted by the editor dialog.
pub struct EditorInput {
    pub title: String,
    pub description: String,
    pub location: String,
    pub category_index: i32,
    pub all_day: bool,
    pub start_date: String,
    pub start_time: String,
    pub end_date: String,
    pub end_time: String,
    pub recurrence_index: i32,
    pub reminder_index: i32,
    /// For recurring events: change the whole series (true) or only this occurrence (false).
    pub apply_to_series: bool,
}

pub struct ImportSummary {
    pub imported: usize,
    pub skipped: usize,
}

pub struct Reminder {
    pub title: String,
    pub start: NaiveDateTime,
    pub minutes_before: i64,
}

/// The event (and occurrence) currently open in the editor.
#[derive(Clone)]
struct Editing {
    id: i64,
    uid: String,
    original_start: Option<NaiveDateTime>,
    recurring: bool,
    original_rule: Option<RecurrenceRule>,
    original_reminder: Option<i64>,
}

pub struct CalendarViewModel {
    repo: Repository,
    view: View,
    anchor: NaiveDate,
    week_days: usize,
    query: String,
    editing: Option<Editing>,
}

pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn key(dt: NaiveDateTime) -> String {
    dt.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn parse_key(text: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S").ok()
}

fn hm(dt: NaiveDateTime) -> String {
    hm_of(dt)
}

/// `HH:MM` of a date-time, as shown in the interface.
pub fn hm_of(dt: NaiveDateTime) -> String {
    format!("{:02}:{:02}", dt.hour(), dt.minute())
}

fn reminder_minutes(index: i32) -> Option<i64> {
    match index {
        1 => Some(0),
        2 => Some(5),
        3 => Some(15),
        4 => Some(30),
        5 => Some(60),
        6 => Some(1440),
        _ => None,
    }
}

fn reminder_index(minutes: Option<i64>) -> i32 {
    match minutes {
        None => 0,
        Some(0) => 1,
        Some(5) => 2,
        Some(15) => 3,
        Some(30) => 4,
        Some(60) => 5,
        Some(1440) => 6,
        Some(_) => REMINDER_KEEP,
    }
}

fn recurrence_freq(index: i32) -> Option<Freq> {
    match index {
        1 => Some(Freq::Daily),
        2 => Some(Freq::Weekly),
        3 => Some(Freq::Monthly),
        4 => Some(Freq::Yearly),
        _ => None,
    }
}

fn recurrence_index(rule: Option<&RecurrenceRule>) -> i32 {
    match rule {
        None => 0,
        Some(r) if *r == RecurrenceRule::simple(r.freq) => match r.freq {
            Freq::Daily => 1,
            Freq::Weekly => 2,
            Freq::Monthly => 3,
            Freq::Yearly => 4,
        },
        Some(_) => RECURRENCE_KEEP,
    }
}

impl CalendarViewModel {
    pub fn new(repo: Repository) -> Self {
        Self {
            repo,
            view: View::Month,
            anchor: today(),
            week_days: 7,
            query: String::new(),
            editing: None,
        }
    }

    pub fn anchor(&self) -> NaiveDate {
        self.anchor
    }

    pub fn set_view(&mut self, view: View) {
        self.view = view;
    }

    /// Number of days shown by the week view, from 1 to 7, chosen from the window width.
    pub fn set_week_days(&mut self, days: usize) {
        self.week_days = days.clamp(1, 7);
    }

    pub fn set_query(&mut self, query: &str) {
        self.query = query.trim().to_string();
    }

    pub fn go_today(&mut self) {
        self.anchor = today();
    }

    /// Moves the anchor by `delta` periods of the current view (months, weeks, days or agenda weeks).
    pub fn navigate(&mut self, delta: i32) {
        match self.view {
            View::Month => {
                let index = i64::from(self.anchor.year()) * 12 + i64::from(self.anchor.month0()) + i64::from(delta);
                let year = index.div_euclid(12) as i32;
                let month = index.rem_euclid(12) as u32 + 1;
                if let Some(date) = clamped_date(year, month, self.anchor.day()) {
                    self.anchor = date;
                }
            }
            View::Week | View::Agenda => self.anchor += Duration::days(7 * i64::from(delta)),
            View::Day => self.anchor += Duration::days(i64::from(delta)),
        }
    }

    /// Selects a day from the month or mini-calendar and switches to the day view.
    pub fn open_day(&mut self, date: NaiveDate) {
        self.anchor = date;
        self.view = View::Day;
    }

    /// Saved theme choice; `None` means the user has not chosen yet.
    pub fn dark_theme(&self) -> Option<bool> {
        crate::settings::dark_theme(&self.repo).ok().flatten()
    }

    pub fn set_dark_theme(&self, dark: bool) -> Result<(), String> {
        crate::settings::set_dark_theme(&self.repo, dark).map_err(err)
    }

    /// One-time import of the settings file from earlier versions.
    pub fn import_legacy_settings(&self, path: &std::path::Path) -> Result<(), String> {
        crate::settings::import_legacy_file(&self.repo, path).map_err(err)
    }

    pub fn categories(&self) -> Result<Vec<Category>, String> {
        self.repo.categories().map_err(err)
    }

    pub fn set_category_visible(&self, id: i64, visible: bool) -> Result<(), String> {
        self.repo.set_category_visible(id, visible).map_err(err)
    }

    /// Occurrences in `[from, to]` from visible categories that match the search query.
    fn visible_occurrences(&self, from: NaiveDate, to: NaiveDate, categories: &[Category]) -> Result<Vec<Occurrence>, String> {
        let visible: HashSet<i64> = categories.iter().filter(|c| c.visible).map(|c| c.id).collect();
        let occurrences = self.repo.occurrences(from, to).map_err(err)?;
        Ok(occurrences
            .into_iter()
            .filter(|o| o.event.id.is_some() && visible.contains(&o.event.category_id))
            .filter(|o| search::matches(&o.event, &self.query))
            .collect())
    }

    pub fn snapshot(&self) -> Result<Snapshot, String> {
        let categories = self.categories()?;
        let accents: HashMap<i64, i32> = categories.iter().map(|c| (c.id, c.accent)).collect();
        let now = today();
        let accent_of = |o: &Occurrence| accents.get(&o.event.category_id).copied().unwrap_or(0);

        let mut snapshot = Snapshot {
            title: String::new(),
            view: self.view,
            month_cells: Vec::new(),
            days: Vec::new(),
            agenda: Vec::new(),
            categories: categories.clone(),
            empty_message: String::new(),
        };

        match self.view {
            View::Month => {
                let grid = month_grid(self.anchor.year(), self.anchor.month());
                let occurrences = self.visible_occurrences(grid[0], grid[41], &categories)?;
                snapshot.title = format!("{} de {}", capitalize(MONTH_NAMES[self.anchor.month0() as usize]), self.anchor.year());
                for date in grid {
                    let day_start = date.and_time(NaiveTime::MIN);
                    let mut items: Vec<&Occurrence> = occurrences
                        .iter()
                        .filter(|o| o.start.date() == date || (o.start.date() < date && o.end > day_start))
                        .collect();
                    items.sort_by_key(|o| (!o.event.all_day, o.start));
                    let shown = items
                        .iter()
                        .take(MONTH_VISIBLE_ITEMS)
                        .map(|o| {
                            let label = if o.event.all_day || o.start.date() < date {
                                o.event.title.clone()
                            } else {
                                format!("{} {}", hm(o.start), o.event.title)
                            };
                            (label, accent_of(o))
                        })
                        .collect();
                    snapshot.month_cells.push(MonthCell {
                        date,
                        label: date.day().to_string(),
                        in_month: date.month() == self.anchor.month(),
                        today: date == now,
                        items: shown,
                        more: items.len().saturating_sub(MONTH_VISIBLE_ITEMS),
                    });
                }
            }
            View::Week | View::Day => {
                let dates: Vec<NaiveDate> = match self.view {
                    View::Day => vec![self.anchor],
                    _ if self.week_days == 7 => {
                        let first = week_start(self.anchor);
                        (0..7).map(|i| first + Duration::days(i)).collect()
                    }
                    _ => (0..self.week_days as i64).map(|i| self.anchor + Duration::days(i)).collect(),
                };
                let (first, last) = (dates[0], dates[dates.len() - 1]);
                let occurrences = self.visible_occurrences(first, last, &categories)?;
                snapshot.title = if dates.len() == 1 {
                    let d = first;
                    format!(
                        "{}, {} de {} de {}",
                        capitalize(WEEKDAY_NAMES[d.weekday().num_days_from_sunday() as usize]),
                        d.day(),
                        MONTH_NAMES[d.month0() as usize],
                        d.year()
                    )
                } else {
                    format!("{} – {}", short_date(first), short_date(last))
                };
                for date in dates {
                    snapshot.days.push(day_column(date, now, &occurrences, &accent_of));
                }
            }
            View::Agenda => {
                let span = if self.query.is_empty() { AGENDA_DAYS } else { 365 };
                let to = self.anchor + Duration::days(span - 1);
                let occurrences = self.visible_occurrences(self.anchor, to, &categories)?;
                snapshot.title = "Agenda".into();
                snapshot.agenda = occurrences
                    .iter()
                    .map(|o| AgendaRow {
                        event_id: o.event.id.unwrap_or_default(),
                        start_key: key(o.start),
                        title: o.event.title.clone(),
                        when: when_text(o),
                        location: o.event.location.clone(),
                        accent: accent_of(o),
                    })
                    .collect();
                if snapshot.agenda.is_empty() {
                    snapshot.empty_message = if self.query.is_empty() {
                        "Nenhum evento nos próximos dias".into()
                    } else {
                        "Nenhum evento encontrado".into()
                    };
                }
            }
        }
        Ok(snapshot)
    }

    // ---- Editor -------------------------------------------------------------------------------

    /// Defaults for a new event: one hour starting at `hour` on `date`.
    pub fn open_new(&mut self, date: NaiveDate, hour: u32, categories: &[Category]) -> EditorData {
        self.editing = None;
        let start = date.and_hms_opt(hour.min(23), 0, 0).unwrap_or_else(|| date.and_time(NaiveTime::MIN));
        let end = start + Duration::hours(1);
        EditorData {
            editing: false,
            recurring: false,
            title: String::new(),
            description: String::new(),
            location: String::new(),
            category_index: categories.iter().position(|c| c.visible).unwrap_or(0) as i32,
            all_day: false,
            start_date: start.format("%Y-%m-%d").to_string(),
            start_time: hm(start),
            end_date: end.format("%Y-%m-%d").to_string(),
            end_time: hm(end),
            recurrence_index: 0,
            reminder_index: 0,
        }
    }

    /// Loads an existing event (or one occurrence of a series) into the editor.
    pub fn open_occurrence(&mut self, event_id: i64, start_key: &str, categories: &[Category]) -> Result<EditorData, String> {
        let event = self.repo.event(event_id).map_err(err)?.ok_or("Evento não encontrado")?;
        let original_start = parse_key(start_key).ok_or("Ocorrência inválida")?;
        let duration = event.duration();
        let (start, end) = if event.rrule.is_some() {
            (original_start, original_start + duration)
        } else {
            (event.start, event.end)
        };
        // Stored end is exclusive; the form shows the inclusive last day.
        let end = if event.all_day { end - Duration::days(1) } else { end };
        let category_index = categories.iter().position(|c| c.id == event.category_id).unwrap_or(0) as i32;

        self.editing = Some(Editing {
            id: event_id,
            uid: event.uid.clone(),
            original_start: Some(original_start),
            recurring: event.rrule.is_some(),
            original_rule: event.rrule.clone(),
            original_reminder: event.reminder_minutes,
        });

        Ok(EditorData {
            editing: true,
            recurring: event.rrule.is_some(),
            title: event.title,
            description: event.description,
            location: event.location,
            category_index,
            all_day: event.all_day,
            start_date: start.format("%Y-%m-%d").to_string(),
            start_time: hm(start),
            end_date: end.format("%Y-%m-%d").to_string(),
            end_time: hm(end),
            recurrence_index: recurrence_index(event.rrule.as_ref()),
            reminder_index: reminder_index(event.reminder_minutes),
        })
    }

    pub fn close_editor(&mut self) {
        self.editing = None;
    }

    /// Creates or updates the event from the form. For a recurring occurrence without
    /// `apply_to_series`, the occurrence is cancelled and the edit becomes a standalone event.
    pub fn save(&mut self, input: &EditorInput, categories: &[Category]) -> Result<(), String> {
        let event = self.build_event(input, categories)?;
        match self.editing.clone() {
            None => {
                self.repo.insert_event(&event).map_err(err)?;
            }
            Some(editing) if editing.recurring && !input.apply_to_series => {
                if let Some(original) = editing.original_start {
                    self.repo.cancel_occurrence(editing.id, original).map_err(err)?;
                }
                self.repo.insert_event(&event).map_err(err)?;
            }
            Some(editing) => {
                let mut event = event;
                event.id = Some(editing.id);
                event.uid = editing.uid;
                self.repo.update_event(&event).map_err(err)?;
            }
        }
        self.editing = None;
        Ok(())
    }

    fn build_event(&self, input: &EditorInput, categories: &[Category]) -> Result<Event, String> {
        let (start, end) = parse_span(input)?;
        let category = categories
            .get(input.category_index.max(0) as usize)
            .ok_or("Selecione uma categoria")?;

        let rrule = match input.recurrence_index {
            0 => None,
            index => match recurrence_freq(index) {
                Some(freq) => Some(RecurrenceRule::simple(freq)),
                None => self.editing.as_ref().and_then(|e| e.original_rule.clone()),
            },
        };
        let reminder_minutes = match input.reminder_index {
            REMINDER_KEEP => self.editing.as_ref().and_then(|e| e.original_reminder),
            index => reminder_minutes(index),
        };

        let event = Event {
            id: None,
            uid: new_uid(),
            title: input.title.trim().to_string(),
            description: input.description.trim().to_string(),
            location: input.location.trim().to_string(),
            category_id: category.id,
            all_day: input.all_day,
            start,
            end,
            rrule,
            reminder_minutes,
        };
        event.validate().map_err(err)?;
        Ok(event)
    }

    /// Deletes the open event, or only the open occurrence when `apply_to_series` is false.
    pub fn delete(&mut self, apply_to_series: bool) -> Result<(), String> {
        let editing = self.editing.clone().ok_or("Nenhum evento aberto")?;
        if editing.recurring && !apply_to_series {
            let original = editing.original_start.ok_or("Ocorrência inválida")?;
            self.repo.cancel_occurrence(editing.id, original).map_err(err)?;
        } else {
            self.repo.delete_event(editing.id).map_err(err)?;
        }
        self.editing = None;
        Ok(())
    }

    /// Copies the open event with a new identity. The copy starts at the same time.
    pub fn duplicate(&mut self) -> Result<(), String> {
        let editing = self.editing.clone().ok_or("Nenhum evento aberto")?;
        let event = self.repo.event(editing.id).map_err(err)?.ok_or("Evento não encontrado")?;
        self.repo.insert_event(&event.duplicate()).map_err(err)?;
        Ok(())
    }

    // ---- Import / export ----------------------------------------------------------------------

    pub fn import_ics(&mut self, text: &str) -> Result<ImportSummary, String> {
        let report = ics::parse(text).map_err(err)?;
        let mut imported = 0;
        for item in report.events {
            let category_id = self.repo.ensure_category(&item.category).map_err(err)?;
            let mut event = item.event;
            event.category_id = category_id;
            // Imported events always get a local UID: the same file can be imported twice, and
            // UIDs are unique in the database. Imports therefore add copies, never overwrite.
            event.uid = new_uid();
            let id = self.repo.insert_event(&event).map_err(err)?;
            for excluded in item.exdates {
                self.repo.cancel_occurrence(id, excluded).map_err(err)?;
            }
            imported += 1;
        }
        Ok(ImportSummary { imported, skipped: report.skipped })
    }

    pub fn export_ics(&self) -> Result<String, String> {
        let categories = self.categories()?;
        let names: HashMap<i64, String> = categories.into_iter().map(|c| (c.id, c.name)).collect();
        let mut items = Vec::new();
        for event in self.repo.events().map_err(err)? {
            let id = event.id.unwrap_or_default();
            let exdates = self.repo.cancellations(id).map_err(err)?;
            let category = names.get(&event.category_id).cloned().unwrap_or_default();
            items.push(IcsEvent { event, category, exdates });
        }
        Ok(ics::write(&items))
    }

    // ---- Reminders ----------------------------------------------------------------------------

    /// Reminders that became due at `now` and were not announced before. Each one is recorded
    /// in the log so it fires only once.
    pub fn due_reminders(&self, now: NaiveDateTime) -> Result<Vec<Reminder>, String> {
        let from = now.date();
        let to = from + Duration::days(1);
        let mut due = Vec::new();
        for occurrence in self.repo.occurrences(from, to).map_err(err)? {
            let Some(minutes) = occurrence.event.reminder_minutes else {
                continue;
            };
            let trigger = occurrence.start - Duration::minutes(minutes);
            // Fire from the trigger time until one minute after the start.
            if trigger <= now && now < occurrence.start + Duration::minutes(1) {
                let id = occurrence.event.id.unwrap_or_default();
                if self.repo.mark_reminder(id, occurrence.start).map_err(err)? {
                    due.push(Reminder {
                        title: occurrence.event.title.clone(),
                        start: occurrence.start,
                        minutes_before: minutes,
                    });
                }
            }
        }
        Ok(due)
    }
}

/// Converts the form's dates and times into the stored `[start, end)` span.
fn parse_span(input: &EditorInput) -> Result<(NaiveDateTime, NaiveDateTime), String> {
    let date = |text: &str, what: &str| {
        NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d")
            .map_err(|_| format!("{what} inválida (use AAAA-MM-DD)"))
    };
    let time = |text: &str, what: &str| {
        NaiveTime::parse_from_str(text.trim(), "%H:%M").map_err(|_| format!("{what} inválida (use HH:MM)"))
    };
    let start_date = date(&input.start_date, "Data inicial")?;
    let end_date = date(&input.end_date, "Data final")?;
    if input.all_day {
        Ok(Event::all_day_span(start_date, end_date))
    } else {
        let start = start_date.and_time(time(&input.start_time, "Hora inicial")?);
        let end = end_date.and_time(time(&input.end_time, "Hora final")?);
        Ok((start, end))
    }
}

fn day_column(date: NaiveDate, now: NaiveDate, occurrences: &[Occurrence], accent_of: &impl Fn(&Occurrence) -> i32) -> DayColumn {
    let day_start = date.and_time(NaiveTime::MIN);
    let day_end = day_start + Duration::days(1);
    let on_day: Vec<&Occurrence> = occurrences
        .iter()
        .filter(|o| o.start < day_end && (o.end > day_start || o.start >= day_start))
        .collect();

    let mut all_day = Vec::new();
    let mut timed: Vec<&Occurrence> = Vec::new();
    for o in on_day {
        if o.event.all_day {
            all_day.push(AllDayChip {
                event_id: o.event.id.unwrap_or_default(),
                start_key: key(o.start),
                title: o.event.title.clone(),
                accent: accent_of(o),
            });
        } else {
            timed.push(o);
        }
    }

    let spans: Vec<(i64, i64)> = timed
        .iter()
        .map(|o| {
            let top = (o.start.max(day_start) - day_start).num_minutes();
            let bottom = (o.end.min(day_end) - day_start).num_minutes();
            let height = (bottom - top).max(MIN_BLOCK_MINUTES).min(24 * 60 - top);
            (top, top + height)
        })
        .collect();
    let slots = layout(&spans);

    let blocks = timed
        .iter()
        .zip(spans.iter())
        .zip(slots.iter())
        .map(|((o, (top, bottom)), slot)| Block {
            event_id: o.event.id.unwrap_or_default(),
            start_key: key(o.start),
            title: o.event.title.clone(),
            subtitle: {
                let mut text = format!("{} – {}", hm(o.start), hm(o.end));
                if !o.event.location.is_empty() {
                    text.push_str(" · ");
                    text.push_str(&o.event.location);
                }
                text
            },
            top_min: *top,
            height_min: bottom - top,
            column: slot.column,
            columns: slot.columns,
            accent: accent_of(o),
        })
        .collect();

    DayColumn {
        date,
        label: format!("{} {}", WEEKDAY_SHORT[date.weekday().num_days_from_sunday() as usize], date.day()),
        today: date == now,
        all_day,
        blocks,
    }
}

fn when_text(o: &Occurrence) -> String {
    let weekday = WEEKDAY_SHORT[o.start.weekday().num_days_from_sunday() as usize];
    let month = &MONTH_NAMES[o.start.month0() as usize][..3];
    if o.event.all_day {
        format!("{weekday} {} {month} · dia inteiro", o.start.day())
    } else {
        format!("{weekday} {} {month} · {} – {}", o.start.day(), hm(o.start), hm(o.end))
    }
}

fn short_date(date: NaiveDate) -> String {
    format!("{:02}/{:02}/{}", date.day(), date.month(), date.year())
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vm() -> CalendarViewModel {
        let repo = Repository::in_memory().unwrap();
        let mut vm = CalendarViewModel::new(repo);
        vm.anchor = NaiveDate::from_ymd_opt(2026, 11, 7).unwrap();
        vm
    }

    fn input(title: &str, start: &str, end: &str) -> EditorInput {
        EditorInput {
            title: title.into(),
            description: String::new(),
            location: String::new(),
            category_index: 0,
            all_day: false,
            start_date: start.split('T').next().unwrap().into(),
            start_time: start.split('T').nth(1).unwrap().into(),
            end_date: end.split('T').next().unwrap().into(),
            end_time: end.split('T').nth(1).unwrap().into(),
            recurrence_index: 0,
            reminder_index: 0,
            apply_to_series: false,
        }
    }

    #[test]
    fn creates_event_and_shows_it_in_month_view() {
        let mut vm = vm();
        let cats = vm.categories().unwrap();
        vm.save(&input("Dentista", "2026-11-10T14:00", "2026-11-10T15:00"), &cats).unwrap();
        let snap = vm.snapshot().unwrap();
        let cell = snap.month_cells.iter().find(|c| c.date == NaiveDate::from_ymd_opt(2026, 11, 10).unwrap()).unwrap();
        assert_eq!(cell.items[0].0, "14:00 Dentista");
    }

    #[test]
    fn invalid_form_is_rejected_with_message() {
        let mut vm = vm();
        let cats = vm.categories().unwrap();
        let err = vm.save(&input("", "2026-11-10T14:00", "2026-11-10T15:00"), &cats).unwrap_err();
        assert!(err.contains("título"));
        let err = vm.save(&input("x", "2026-13-10T14:00", "2026-11-10T15:00"), &cats).unwrap_err();
        assert!(err.contains("Data inicial"));
    }

    #[test]
    fn editing_one_occurrence_cancels_it_and_creates_standalone_event() {
        let mut vm = vm();
        let cats = vm.categories().unwrap();
        let mut series = input("Academia", "2026-11-02T07:00", "2026-11-02T08:00");
        series.recurrence_index = 1; // daily
        vm.save(&series, &cats).unwrap();
        let id = vm.repo.events().unwrap()[0].id.unwrap();

        vm.open_occurrence(id, "2026-11-05T07:00:00", &cats).unwrap();
        let mut edit = input("Academia (mudou)", "2026-11-05T09:00", "2026-11-05T10:00");
        edit.recurrence_index = 0;
        vm.save(&edit, &cats).unwrap();

        let from = NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
        let to = NaiveDate::from_ymd_opt(2026, 11, 6).unwrap();
        let titles: Vec<String> = vm.repo.occurrences(from, to).unwrap().into_iter().map(|o| o.event.title).collect();
        // Days 2, 3, 4 and 6 remain; day 5 was cancelled.
        assert_eq!(titles.iter().filter(|t| *t == "Academia").count(), 4);
        assert_eq!(titles.iter().filter(|t| *t == "Academia (mudou)").count(), 1);
    }

    #[test]
    fn deleting_whole_series_removes_every_occurrence() {
        let mut vm = vm();
        let cats = vm.categories().unwrap();
        let mut series = input("Aula", "2026-11-02T19:00", "2026-11-02T20:00");
        series.recurrence_index = 2;
        vm.save(&series, &cats).unwrap();
        let id = vm.repo.events().unwrap()[0].id.unwrap();
        vm.open_occurrence(id, "2026-11-09T19:00:00", &cats).unwrap();
        vm.delete(true).unwrap();
        assert!(vm.repo.events().unwrap().is_empty());
    }

    #[test]
    fn navigation_clamps_month_days() {
        let mut vm = vm();
        vm.anchor = NaiveDate::from_ymd_opt(2026, 1, 31).unwrap();
        vm.navigate(1);
        assert_eq!(vm.anchor, NaiveDate::from_ymd_opt(2026, 2, 28).unwrap());
        vm.navigate(-2);
        assert_eq!(vm.anchor, NaiveDate::from_ymd_opt(2025, 12, 28).unwrap());
    }

    #[test]
    fn search_filters_every_view() {
        let mut vm = vm();
        let cats = vm.categories().unwrap();
        vm.save(&input("Reunião", "2026-11-10T09:00", "2026-11-10T10:00"), &cats).unwrap();
        vm.save(&input("Dentista", "2026-11-11T09:00", "2026-11-11T10:00"), &cats).unwrap();
        vm.set_view(View::Agenda);
        vm.set_query("reuniao");
        let snap = vm.snapshot().unwrap();
        assert_eq!(snap.agenda.len(), 1);
        assert_eq!(snap.agenda[0].title, "Reunião");
    }

    #[test]
    fn hidden_category_is_excluded() {
        let mut vm = vm();
        let cats = vm.categories().unwrap();
        vm.save(&input("Consulta", "2026-11-10T09:00", "2026-11-10T10:00"), &cats).unwrap();
        vm.set_category_visible(cats[0].id, false).unwrap();
        vm.set_view(View::Agenda);
        assert!(vm.snapshot().unwrap().agenda.is_empty());
    }

    #[test]
    fn week_view_places_overlapping_blocks_side_by_side() {
        let mut vm = vm();
        let cats = vm.categories().unwrap();
        // The test anchor is 7 Nov 2026 (Saturday), so its Sunday-first week is 1-7 Nov.
        vm.save(&input("A", "2026-11-07T10:00", "2026-11-07T11:00"), &cats).unwrap();
        vm.save(&input("B", "2026-11-07T10:30", "2026-11-07T11:30"), &cats).unwrap();
        vm.set_view(View::Week);
        let snap = vm.snapshot().unwrap();
        let column = snap.days.iter().find(|d| d.date == NaiveDate::from_ymd_opt(2026, 11, 7).unwrap()).unwrap();
        assert_eq!(column.blocks.len(), 2);
        assert!(column.blocks.iter().all(|b| b.columns == 2));
    }

    #[test]
    fn reminders_fire_once_inside_their_window() {
        let mut vm = vm();
        let cats = vm.categories().unwrap();
        let mut event = input("Pagar boleto", "2026-11-10T09:00", "2026-11-10T09:30");
        event.reminder_index = 3; // 15 minutes before
        vm.save(&event, &cats).unwrap();
        let before = NaiveDate::from_ymd_opt(2026, 11, 10).unwrap().and_hms_opt(8, 40, 0).unwrap();
        let inside = NaiveDate::from_ymd_opt(2026, 11, 10).unwrap().and_hms_opt(8, 50, 0).unwrap();
        assert!(vm.due_reminders(before).unwrap().is_empty());
        assert_eq!(vm.due_reminders(inside).unwrap().len(), 1);
        assert!(vm.due_reminders(inside).unwrap().is_empty());
    }

    #[test]
    fn ics_export_then_import_into_new_database() {
        let mut vm = vm();
        let cats = vm.categories().unwrap();
        vm.save(&input("Viagem", "2026-11-10T09:00", "2026-11-10T10:00"), &cats).unwrap();
        let text = vm.export_ics().unwrap();

        let mut other = CalendarViewModel::new(Repository::in_memory().unwrap());
        let summary = other.import_ics(&text).unwrap();
        assert_eq!(summary.imported, 1);
        assert_eq!(summary.skipped, 0);
        assert_eq!(other.repo.events().unwrap()[0].title, "Viagem");
    }

    #[test]
    fn malformed_ics_is_reported_not_panicked() {
        let mut vm = vm();
        assert!(vm.import_ics("BEGIN:VEVENT\nSUMMARY:x\n").is_err());
    }
}
