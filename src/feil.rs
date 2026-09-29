//! Feiltyper og utgangskoder (SPEC §8).

use std::path::PathBuf;
use std::process::ExitCode;

/// Utgangskodene fra SPEC §8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Utgangskode {
    /// Vellykket, også når søket ga null treff.
    Ok = 0,
    /// Bruksfeil eller fatal feil.
    Feil = 1,
    /// `--streng` er satt, og minst én kilde feilet eller ga utdaterte data.
    Streng = 2,
    /// Ingen data tilgjengelig i det hele tatt.
    IngenData = 3,
}

impl From<Utgangskode> for ExitCode {
    fn from(kode: Utgangskode) -> Self {
        ExitCode::from(kode as u8)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AppFeil {
    #[error("ingen lokale prisdata ennå – kjør `databrus oppdater`, eller søk uten --frakoblet")]
    IngenData,

    #[error("ikke implementert ennå ({0})")]
    IkkeImplementert(&'static str),

    #[error("alle kilder feilet")]
    AlleKilderFeilet,

    #[error("{0}")]
    Bruk(String),

    #[error("konfigurasjon: {0}")]
    Konfig(String),

    #[error("katalog: {0}")]
    Katalog(String),

    #[error("database {}: {melding}", sti.display())]
    Lager { sti: PathBuf, melding: String },

    #[error("nettverk: {0}")]
    Nettverk(String),

    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
}

impl AppFeil {
    pub fn utgangskode(&self) -> Utgangskode {
        match self {
            AppFeil::IngenData => Utgangskode::IngenData,
            _ => Utgangskode::Feil,
        }
    }
}
