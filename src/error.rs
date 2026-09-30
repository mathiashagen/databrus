//! Error types and exit codes (SPEC §8). Error messages are shown to the user and are
//! therefore Norwegian.

use std::path::PathBuf;
use std::process::ExitCode;

/// The exit codes from SPEC §8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExitStatus {
    /// Success, including a search with zero hits.
    Ok = 0,
    /// Usage error or fatal error.
    Error = 1,
    /// `--streng` is set and at least one source failed or served stale data.
    Strict = 2,
    /// No data available at all.
    NoData = 3,
}

impl From<ExitStatus> for ExitCode {
    fn from(status: ExitStatus) -> Self {
        ExitCode::from(status as u8)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("ingen lokale prisdata ennå – kjør `databrus oppdater`, eller søk uten --frakoblet")]
    NoData,

    #[error("ikke implementert ennå ({0})")]
    NotImplemented(&'static str),

    #[error("alle kilder feilet")]
    AllSourcesFailed,

    #[error("{0}")]
    Usage(String),

    #[error("konfigurasjon: {0}")]
    Config(String),

    #[error("katalog: {0}")]
    Catalog(String),

    #[error("database {}: {message}", path.display())]
    Database { path: PathBuf, message: String },

    #[error("planlegging: {0}")]
    Schedule(String),

    #[error("nettverk: {0}")]
    Network(String),

    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
}

impl AppError {
    pub fn exit_status(&self) -> ExitStatus {
        match self {
            AppError::NoData => ExitStatus::NoData,
            _ => ExitStatus::Error,
        }
    }
}
