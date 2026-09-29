//! Én modul per kommando. Hver har en `kjor`-funksjon som returnerer en utgangskode.

pub mod butikker;
pub mod eksporter;
pub mod historikk;
pub mod konfig;
pub mod oppdater;
pub mod overvak;
pub mod planlegg;
pub mod produkter;
pub mod sok;
pub mod tilbud;

use crate::feil::AppFeil;
use crate::lager::Lager;

/// Kommandoer som viser priser gir kode 3 når det ikke finnes noen (SPEC §8).
pub(crate) fn krev_prisdata(lager: &Lager) -> Result<(), AppFeil> {
    if lager.har_prisdata()? {
        Ok(())
    } else {
        Err(AppFeil::IngenData)
    }
}
