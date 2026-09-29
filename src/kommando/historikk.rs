//! `databrus historikk <produkt>` – leser bare lokale data.

use crate::Kontekst;
use crate::cli::HistorikkArgs;
use crate::feil::{AppFeil, Utgangskode};

pub fn kjor(_args: &HistorikkArgs, ktx: &Kontekst) -> Result<Utgangskode, AppFeil> {
    let lager = ktx.apne_lager()?;
    super::krev_prisdata(&lager)?;

    // M2: graf per kjede og tabell med nå/L30/M90/ATL.
    Err(AppFeil::IkkeImplementert("historikkgraf, M2"))
}
