//! Planlagt daglig henting (SPEC §7.5): Oppgaveplanlegging på Windows, systemd/cron på
//! Linux og launchd på macOS. Implementeres i M4.

#[cfg(not(windows))]
mod unix;
#[cfg(windows)]
mod windows;

use jiff::civil::Time;

use crate::feil::AppFeil;

/// Navnet på den planlagte oppgaven.
pub const OPPGAVENAVN: &str = "databrus-oppdater";

pub trait Planlegger {
    fn installer(&self, tid: Time) -> Result<(), AppFeil>;
    fn fjern(&self) -> Result<(), AppFeil>;
    /// En lesbar beskrivelse av den installerte oppgaven, eller `None` hvis ingen finnes.
    fn status(&self) -> Result<Option<String>, AppFeil>;
}

pub fn for_plattform() -> Box<dyn Planlegger> {
    #[cfg(windows)]
    {
        Box::new(windows::Oppgaveplanlegging)
    }
    #[cfg(not(windows))]
    {
        Box::new(unix::Unix)
    }
}
