//! Søk (standardkommandoen).

use crate::Kontekst;
use crate::cli::SokArgs;
use crate::feil::{AppFeil, Utgangskode};
use crate::kilder;
use crate::sok::Sokefilter;

pub async fn kjor(args: &SokArgs, ktx: &Kontekst) -> Result<Utgangskode, AppFeil> {
    let _filter = Sokefilter::fra_args(args, &ktx.konfig);
    let katalog = ktx.katalog()?;
    let mut lager = ktx.apne_lager()?;
    let _kilder =
        kilder::oppfrisk(&mut lager, &ktx.konfig, &katalog, ktx.hentemodus(), &[]).await?;
    super::krev_prisdata(&lager)?;

    // M1: matching → prisberegning → rangering → tabell/JSON, og --streng.
    Err(AppFeil::IkkeImplementert(
        "rangering og visning av priser, M1",
    ))
}
