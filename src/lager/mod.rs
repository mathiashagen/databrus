//! Lokal lagring i SQLite (SPEC §7.2).
//!
//! Tidspunkt lagres som unix-sekunder (UTC). Databasen åpnes i WAL-modus, slik at en
//! planlagt henting og et interaktivt søk kan kjøre samtidig.

pub mod les;
pub mod migrering;
pub mod skriv;

use std::fmt::Display;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::Connection;

pub use les::{Hentelogg, Umatchet};
pub use skriv::{Endring, Prisobservasjon};

use crate::feil::AppFeil;

#[derive(Debug)]
pub struct Lager {
    conn: Connection,
    sti: PathBuf,
}

impl Lager {
    /// Åpner (og oppretter ved behov) databasen og kjører migreringene.
    pub fn apne(sti: &Path) -> Result<Self, AppFeil> {
        if let Some(mappe) = sti.parent()
            && !mappe.as_os_str().is_empty()
        {
            fs::create_dir_all(mappe)?;
        }
        let mut conn = Connection::open(sti).map_err(|e| feil(sti, e))?;
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |rad| rad.get::<_, String>(0))
            .map_err(|e| feil(sti, e))?;
        conn.pragma_update(None, "foreign_keys", true)
            .map_err(|e| feil(sti, e))?;
        conn.busy_timeout(Duration::from_secs(5))
            .map_err(|e| feil(sti, e))?;
        migrering::migrer(&mut conn).map_err(|e| feil(sti, e))?;
        Ok(Self {
            conn,
            sti: sti.to_path_buf(),
        })
    }

    pub fn sti(&self) -> &Path {
        &self.sti
    }
}

/// Databasefeil nevner alltid filen, slik at brukeren vet hva som er galt (SPEC §8).
pub(crate) fn feil(sti: &Path, feil: impl Display) -> AppFeil {
    AppFeil::Lager {
        sti: sti.to_path_buf(),
        melding: feil.to_string(),
    }
}
