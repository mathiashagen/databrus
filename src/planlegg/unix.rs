//! Linux (systemd-brukertimer, ellers crontab) og macOS (launchd).

use jiff::civil::Time;

use super::Planlegger;
use crate::feil::AppFeil;

pub struct Unix;

impl Planlegger for Unix {
    fn installer(&self, _tid: Time) -> Result<(), AppFeil> {
        Err(AppFeil::IkkeImplementert(
            "planlagt henting med systemd/cron/launchd, M4",
        ))
    }

    fn fjern(&self) -> Result<(), AppFeil> {
        Err(AppFeil::IkkeImplementert(
            "planlagt henting med systemd/cron/launchd, M4",
        ))
    }

    fn status(&self) -> Result<Option<String>, AppFeil> {
        Err(AppFeil::IkkeImplementert(
            "planlagt henting med systemd/cron/launchd, M4",
        ))
    }
}
