//! Systems without a supported scheduler (e.g. the BSDs).

use super::{Installed, Job, Scheduler};
use crate::error::AppError;

pub struct Unsupported;

fn unsupported() -> AppError {
    AppError::Schedule(
        "planlagt henting støttes bare på Windows, Linux og macOS – kjør `databrus oppdater --stille` fra cron selv".into(),
    )
}

impl Scheduler for Unsupported {
    fn install(&self, _job: &Job) -> Result<Vec<String>, AppError> {
        Err(unsupported())
    }

    fn remove(&self) -> Result<bool, AppError> {
        Err(unsupported())
    }

    fn status(&self) -> Result<Option<Installed>, AppError> {
        Err(unsupported())
    }
}
