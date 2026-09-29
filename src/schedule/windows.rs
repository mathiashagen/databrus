//! Windows Task Scheduler via `schtasks.exe`.

use jiff::civil::Time;

use super::Scheduler;
use crate::error::AppError;

const NOT_YET: &str = "planlagt henting med schtasks, M4";

pub struct TaskScheduler;

impl Scheduler for TaskScheduler {
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
