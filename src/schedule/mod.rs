//! Scheduled daily fetching (SPEC §7.5): Task Scheduler on Windows, systemd/cron on
//! Linux and launchd on macOS.

#[cfg(not(windows))]
mod unix;
#[cfg(windows)]
mod windows;

use std::path::{Path, PathBuf};

use jiff::civil::Time;

use crate::error::AppError;

/// The name of the scheduled task.
pub const TASK_NAME: &str = "databrus-oppdater";

/// What the scheduled task runs: `databrus oppdater --stille` at `time` every day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub time: Time,
    pub program: PathBuf,
    pub args: Vec<String>,
}

impl Job {
    /// A job that runs this executable. A config file given with `--konfig` is passed on,
    /// since the task doesn't see the shell's arguments or environment.
    pub fn for_current_exe(time: Time, config_file: Option<&Path>) -> Result<Self, AppError> {
        let program = std::env::current_exe()?;
        let mut args = vec!["oppdater".to_owned(), "--stille".to_owned()];
        if let Some(path) = config_file {
            let path = std::path::absolute(path)?;
            let path = path.to_str().ok_or_else(|| {
                AppError::Usage(format!(
                    "konfigurasjonsstien {} er ikke gyldig UTF-8",
                    path.display()
                ))
            })?;
            args.extend(["--konfig".to_owned(), path.to_owned()]);
        }
        Ok(Self {
            time,
            program,
            args,
        })
    }
}

/// An installed task, as read back from the platform's scheduler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    /// The daily start time, if it could be read.
    pub time: Option<Time>,
    /// The command line the task runs.
    pub command: String,
}

pub trait Scheduler {
    /// Installs the job, replacing an existing task with the same name.
    fn install(&self, job: &Job) -> Result<(), AppError>;
    /// Removes the task. Returns `false` if there was none.
    fn remove(&self) -> Result<bool, AppError>;
    /// The installed task, or `None` if there is none.
    fn status(&self) -> Result<Option<Installed>, AppError>;
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
