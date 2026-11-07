//! Schema migrations, applied in order by `rusqlite_migration`, which tracks progress in
//! `PRAGMA user_version`. Databases created by earlier versions keep their version and only
//! the newer steps run.

use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};

pub fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up(include_str!("../../migrations/0001_init.sql")),
        M::up(include_str!("../../migrations/0002_reminders_log.sql")),
        M::up(include_str!("../../migrations/0003_settings.sql")),
    ])
}

pub fn migrate(conn: &mut Connection) -> Result<(), rusqlite_migration::Error> {
    migrations().to_latest(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Repository;

    #[test]
    fn database_at_version_two_only_gains_settings() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("oxide-cal-legacy-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("calendar.db");
        {
            // Database as written by the previous version: steps 1 and 2 applied, user_version 2.
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(include_str!("../../migrations/0001_init.sql")).unwrap();
            conn.execute_batch(include_str!("../../migrations/0002_reminders_log.sql")).unwrap();
            conn.execute("INSERT INTO categories (name, accent) VALUES ('Projetos', 6)", []).unwrap();
            conn.pragma_update(None, "user_version", 2).unwrap();
        }

        let repo = Repository::open(&path).unwrap();
        let version: i64 = Connection::open(&path)
            .unwrap()
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 3);
        // Existing data is kept: the three seeded categories plus the one added above.
        assert_eq!(repo.categories().unwrap().len(), 4);
        // The new step is there.
        repo.set_setting("probe", "ok").unwrap();
        assert_eq!(repo.get_setting("probe").unwrap().as_deref(), Some("ok"));
        drop(repo);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn migrations_are_valid() {
        assert!(migrations().validate().is_ok());
    }
}
