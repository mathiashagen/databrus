//! macOS: a launch agent in `~/Library/LaunchAgents`, loaded into the user's GUI session
//! so desktop notifications can show.

use std::fs;
use std::path::PathBuf;

use directories::BaseDirs;

use super::formats::{self, LAUNCHD_LABEL};
use super::{Installed, Job, Scheduler, run};
use crate::error::AppError;

pub struct Launchd;

impl Scheduler for Launchd {
    fn install(&self, job: &Job) -> Result<Vec<String>, AppError> {
        let path = plist_path()?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        if let Some(dir) = job.log_file.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&path, formats::launchd_plist(job))?;

        let domain = gui_domain()?;
        // An older version of the agent may be loaded; unloading fails if it is not.
        let _ = run(
            "launchctl",
            &["bootout", &format!("{domain}/{LAUNCHD_LABEL}")],
            "",
        );
        run(
            "launchctl",
            &["bootstrap", &domain, &path.to_string_lossy()],
            "kunne ikke laste inn launchd-agenten",
        )?;
        Ok(vec![
            "kjører som launchd-agent; en henting som ble hoppet over mens Mac-en sov, kjøres \
             når den våkner"
                .into(),
            format!("logg: {}", job.log_file.display()),
        ])
    }

    fn remove(&self) -> Result<bool, AppError> {
        let path = plist_path()?;
        if !path.exists() {
            return Ok(false);
        }
        let domain = gui_domain()?;
        let _ = run(
            "launchctl",
            &["bootout", &format!("{domain}/{LAUNCHD_LABEL}")],
            "",
        );
        fs::remove_file(&path)?;
        Ok(true)
    }

    fn status(&self) -> Result<Option<Installed>, AppError> {
        match fs::read_to_string(plist_path()?) {
            Ok(plist) => Ok(Some(formats::parse_plist(&plist))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

/// `~/Library/LaunchAgents/<label>.plist`
fn plist_path() -> Result<PathBuf, AppError> {
    BaseDirs::new()
        .map(|b| {
            b.home_dir()
                .join("Library")
                .join("LaunchAgents")
                .join(format!("{LAUNCHD_LABEL}.plist"))
        })
        .ok_or_else(|| AppError::Schedule("fant ikke hjemmemappen".into()))
}

/// `gui/<uid>`: the user's login session.
fn gui_domain() -> Result<String, AppError> {
    let output = run("id", &["-u"], "kunne ikke finne bruker-id-en")?;
    let uid = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok(format!("gui/{uid}"))
}
