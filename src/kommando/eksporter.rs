//! `databrus eksporter` – leser bare lokale data.

use crate::Kontekst;
use crate::cli::EksporterArgs;
use crate::feil::{AppFeil, Utgangskode};

pub fn kjor(_args: &EksporterArgs, ktx: &Kontekst) -> Result<Utgangskode, AppFeil> {
    let lager = ktx.apne_lager()?;
    super::krev_prisdata(&lager)?;

    // M2: CSV med ett prisintervall per rad.
    Err(AppFeil::IkkeImplementert("CSV-eksport, M2"))
}
