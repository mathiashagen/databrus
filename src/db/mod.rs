//! Local storage in SQLite (SPEC §7.2).
//!
//! Timestamps are stored as unix seconds (UTC). The database is opened in WAL mode, so a
//! scheduled fetch and an interactive search can run at the same time.

pub mod alerts;
pub mod migrations;
pub mod read;
pub mod write;

use std::fmt::Display;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::Connection;

pub use read::{FetchLog, HistoricPrice, StoredPrice, UnmatchedListing};
pub use write::{Change, PriceObservation};

use crate::error::AppError;

#[derive(Debug)]
pub struct Database {
    conn: Connection,
    path: PathBuf,
}

impl Database {
    /// Opens (and creates if needed) the database and runs the migrations.
    pub fn open(path: &Path) -> Result<Self, AppError> {
        if let Some(dir) = path.parent()
            && !dir.as_os_str().is_empty()
        {
            fs::create_dir_all(dir)?;
        }
        let mut conn = Connection::open(path).map_err(|e| error(path, e))?;
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get::<_, String>(0))
            .map_err(|e| error(path, e))?;
        conn.pragma_update(None, "foreign_keys", true)
            .map_err(|e| error(path, e))?;
        conn.busy_timeout(Duration::from_secs(5))
            .map_err(|e| error(path, e))?;
        reject_prerelease_schema(&conn, path)?;
        migrations::migrate(&mut conn).map_err(|e| error(path, e))?;
        Ok(Self {
            conn,
            path: path.to_path_buf(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Before the first release, the schema used Norwegian table names and migration 001 was
/// rewritten in English. Such a database only holds re-fetchable data; say so plainly
/// instead of failing later with "no such table".
fn reject_prerelease_schema(conn: &Connection, path: &Path) -> Result<(), AppError> {
    let old: bool = conn
        .query_row(
            "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'oppforing')",
            [],
            |row| row.get(0),
        )
        .map_err(|e| error(path, e))?;
    if old {
        return Err(error(
            path,
            "databasen er fra en utviklingsversjon med et annet skjema – slett filen, \
             så opprettes den på nytt ved neste henting",
        ));
    }
    Ok(())
}

/// Database errors always name the file, so the user knows what is wrong (SPEC §8).
pub(crate) fn error(path: &Path, error: impl Display) -> AppError {
    AppError::Database {
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prerelease_schema_gets_a_clear_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("databrus.sqlite");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch("CREATE TABLE oppforing (id INTEGER); PRAGMA user_version = 1;")
                .unwrap();
        }
        let message = Database::open(&path).unwrap_err().to_string();
        assert!(message.contains("utviklingsversjon"), "{message}");
    }
}
