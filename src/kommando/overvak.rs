//! `databrus overvak …` – prisvarsler.

use crate::Kontekst;
use crate::cli::OvervakKommando;
use crate::feil::{AppFeil, Utgangskode};

pub fn kjor(_kommando: &OvervakKommando, _ktx: &Kontekst) -> Result<Utgangskode, AppFeil> {
    Err(AppFeil::IkkeImplementert("prisvarsler, M4"))
}
