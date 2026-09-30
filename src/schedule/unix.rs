//! Linux (systemd user timer, otherwise crontab) and macOS (launchd).

use super::{Installed, Job, Scheduler};
use crate::error::AppError;

const NOT_YET: &str = "planlagt henting med systemd/cron/launchd, M4";

pub struct Unix;

impl Scheduler for Unix {
    fn install(&self, _job: &Job) -> Result<(), AppError> {
        Err(AppError::NotImplemented(NOT_YET))
    }

    fn remove(&self) -> Result<bool, AppError> {
        Err(AppError::NotImplemented(NOT_YET))
    }

    fn status(&self) -> Result<Option<Installed>, AppError> {
        Err(AppError::NotImplemented(NOT_YET))
    }
}
