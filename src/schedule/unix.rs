//! Linux (systemd user timer, otherwise crontab) and macOS (launchd).

use jiff::civil::Time;

use super::Scheduler;
use crate::error::AppError;

const NOT_YET: &str = "planlagt henting med systemd/cron/launchd, M4";

pub struct Unix;

impl Scheduler for Unix {
    fn install(&self, _time: Time) -> Result<(), AppError> {
        Err(AppError::NotImplemented(NOT_YET))
    }

    fn remove(&self) -> Result<(), AppError> {
        Err(AppError::NotImplemented(NOT_YET))
    }

    fn status(&self) -> Result<Option<String>, AppError> {
        Err(AppError::NotImplemented(NOT_YET))
    }
}
