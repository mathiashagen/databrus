//! Scheduled daily fetching (SPEC §7.5): Task Scheduler on Windows, systemd/cron on
//! Linux and launchd on macOS. Implemented in M4.

#[cfg(not(windows))]
mod unix;
#[cfg(windows)]
mod windows;

use jiff::civil::Time;

use crate::error::AppError;

/// The name of the scheduled task.
pub const TASK_NAME: &str = "databrus-oppdater";

pub trait Scheduler {
    fn install(&self, time: Time) -> Result<(), AppError>;
    fn remove(&self) -> Result<(), AppError>;
    /// A readable description of the installed task, or `None` if there is none.
    fn status(&self) -> Result<Option<String>, AppError>;
}

pub fn for_platform() -> Box<dyn Scheduler> {
    #[cfg(windows)]
    {
        Box::new(windows::TaskScheduler)
    }
    #[cfg(not(windows))]
    {
        Box::new(unix::Unix)
    }
}
