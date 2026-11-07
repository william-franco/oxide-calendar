//! Local persistence over SQLite. The rest of the app talks to `Repository`, never to SQL.

mod migrations;
mod repository;

pub use repository::{Repository, StorageError};
