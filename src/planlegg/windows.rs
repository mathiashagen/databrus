//! Windows Oppgaveplanlegging via `schtasks.exe`.

use jiff::civil::Time;

use super::Planlegger;
use crate::feil::AppFeil;

pub struct Oppgaveplanlegging;

impl Planlegger for Oppgaveplanlegging {
    fn installer(&self, _tid: Time) -> Result<(), AppFeil> {
        Err(AppFeil::IkkeImplementert(
            "planlagt henting med schtasks, M4",
        ))
    }

    fn fjern(&self) -> Result<(), AppFeil> {
        Err(AppFeil::IkkeImplementert(
            "planlagt henting med schtasks, M4",
        ))
    }

    fn status(&self) -> Result<Option<String>, AppFeil> {
        Err(AppFeil::IkkeImplementert(
            "planlagt henting med schtasks, M4",
        ))
    }
}
