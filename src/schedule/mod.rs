//! Scheduled daily fetching (SPEC §7.5): Task Scheduler on Windows, a systemd user timer
//! or cron on Linux and a launch agent on macOS.

mod formats;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod other;
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
    /// Where cron and launchd write the job's output (errors and alerts), since those
    /// schedulers have nowhere else to put it. Task Scheduler and systemd do not use it.
    pub log_file: PathBuf,
}

impl Job {
    /// A job that runs this executable. A config file given with `--konfig` is passed on,
    /// since the task doesn't see the shell's arguments or environment.
    pub fn for_current_exe(
        time: Time,
        config_file: Option<&Path>,
        log_file: PathBuf,
    ) -> Result<Self, AppError> {
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
            log_file,
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
    /// Installs the job, replacing an existing task with the same name. Returns notes
    /// for the user about how the platform runs it.
    fn install(&self, job: &Job) -> Result<Vec<String>, AppError>;
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
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::Linux)
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::Launchd)
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        Box::new(other::Unsupported)
    }
}

/// Runs a scheduler's command line tool, with its error output in the error.
#[cfg(not(windows))]
fn run(program: &str, args: &[&str], what: &str) -> Result<std::process::Output, AppError> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .map_err(|e| AppError::Schedule(format!("kunne ikke starte {program}: {e}")))?;
    if output.status.success() {
        Ok(output)
    } else {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(AppError::Schedule(if message.is_empty() {
            format!("{what} ({program} {})", output.status)
        } else {
            format!("{what}: {message}")
        }))
    }
}
