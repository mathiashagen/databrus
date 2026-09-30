//! Linux: a systemd user timer when the user has a systemd session, otherwise a crontab
//! line. Only one of them is installed at a time.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use directories::BaseDirs;

use super::formats::{self, SYSTEMD_SERVICE, SYSTEMD_TIMER};
use super::{Installed, Job, Scheduler, run};
use crate::error::AppError;

pub struct Linux;

impl Scheduler for Linux {
    fn install(&self, job: &Job) -> Result<Vec<String>, AppError> {
        if systemd_available() {
            remove_cron()?;
            let dir = unit_dir()?;
            fs::create_dir_all(&dir)?;
            fs::write(dir.join(SYSTEMD_SERVICE), formats::systemd_service(job))?;
            fs::write(dir.join(SYSTEMD_TIMER), formats::systemd_timer(job))?;
            systemctl(&["daemon-reload"], "kunne ikke laste inn systemd-enhetene")?;
            systemctl(
                &["enable", "--now", SYSTEMD_TIMER],
                "kunne ikke starte systemd-timeren",
            )?;
            Ok(vec![
                "kjører som systemd-timer for brukeren din, og tar igjen en henting som ble \
                 hoppet over mens maskinen var av"
                    .into(),
                "med `loginctl enable-linger` kjører den også når du ikke er logget inn".into(),
            ])
        } else {
            remove_systemd()?;
            if let Some(dir) = job.log_file.parent() {
                fs::create_dir_all(dir)?;
            }
            let (crontab, _) =
                formats::crontab_with(&read_crontab()?, Some(formats::cron_line(job).as_str()));
            write_crontab(&crontab)?;
            Ok(vec![
                "kjører fra crontab, siden systemd ikke er tilgjengelig; cron tar ikke igjen en \
                 henting som ble hoppet over mens maskinen var av"
                    .into(),
                format!("logg: {}", job.log_file.display()),
            ])
        }
    }

    fn remove(&self) -> Result<bool, AppError> {
        let systemd = remove_systemd()?;
        let cron = remove_cron()?;
        Ok(systemd || cron)
    }

    fn status(&self) -> Result<Option<Installed>, AppError> {
        let dir = unit_dir()?;
        let service = fs::read_to_string(dir.join(SYSTEMD_SERVICE));
        let timer = fs::read_to_string(dir.join(SYSTEMD_TIMER));
        if let (Ok(service), Ok(timer)) = (service, timer) {
            return Ok(Some(formats::parse_systemd(&service, &timer)));
        }
        if !crontab_available() {
            return Ok(None);
        }
        Ok(formats::parse_cron(&read_crontab()?))
    }
}

/// Whether the user has a running systemd user manager.
fn systemd_available() -> bool {
    Command::new("systemctl")
        .args(["--user", "show-environment"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn systemctl(args: &[&str], what: &str) -> Result<(), AppError> {
    let args: Vec<&str> = std::iter::once("--user")
        .chain(args.iter().copied())
        .collect();
    run("systemctl", &args, what).map(|_| ())
}

/// `~/.config/systemd/user` (or under `$XDG_CONFIG_HOME`).
fn unit_dir() -> Result<PathBuf, AppError> {
    BaseDirs::new()
        .map(|b| b.config_dir().join("systemd").join("user"))
        .ok_or_else(|| AppError::Schedule("fant ikke hjemmemappen".into()))
}

fn remove_systemd() -> Result<bool, AppError> {
    let dir = unit_dir()?;
    let (timer, service) = (dir.join(SYSTEMD_TIMER), dir.join(SYSTEMD_SERVICE));
    if !timer.exists() && !service.exists() {
        return Ok(false);
    }
    let running = systemd_available();
    if running {
        // Fails if it is already stopped, which is fine.
        let _ = systemctl(&["disable", "--now", SYSTEMD_TIMER], "");
    }
    remove_if_present(&timer)?;
    remove_if_present(&service)?;
    if running {
        systemctl(&["daemon-reload"], "kunne ikke laste inn systemd-enhetene")?;
    }
    Ok(true)
}

fn remove_if_present(path: &Path) -> Result<(), AppError> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

fn crontab_available() -> bool {
    Command::new("crontab")
        .arg("-l")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

fn remove_cron() -> Result<bool, AppError> {
    if !crontab_available() {
        return Ok(false);
    }
    let (crontab, found) = formats::crontab_with(&read_crontab()?, None);
    if found {
        write_crontab(&crontab)?;
    }
    Ok(found)
}

/// The user's crontab. `crontab -l` fails when there is none; that is the only failure
/// read as empty, since writing back would otherwise wipe the user's other jobs.
fn read_crontab() -> Result<String, AppError> {
    let output = Command::new("crontab")
        .arg("-l")
        .output()
        .map_err(|e| AppError::Schedule(format!("fant verken systemd eller crontab ({e})")))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let message = String::from_utf8_lossy(&output.stderr);
    if message.contains("no crontab") {
        Ok(String::new())
    } else {
        Err(AppError::Schedule(format!(
            "kunne ikke lese crontab: {}",
            message.trim()
        )))
    }
}

fn write_crontab(crontab: &str) -> Result<(), AppError> {
    let mut child = Command::new("crontab")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AppError::Schedule(format!("kunne ikke starte crontab: {e}")))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(crontab.as_bytes())?;
    }
    let output = child.wait_with_output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(AppError::Schedule(format!(
            "kunne ikke skrive crontab: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}
