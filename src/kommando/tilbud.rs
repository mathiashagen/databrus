//! `databrus tilbud`

use crate::Kontekst;
use crate::cli::TilbudArgs;
use crate::feil::{AppFeil, Utgangskode};
use crate::kilder;
use crate::sok::Sokefilter;

pub async fn kjor(args: &TilbudArgs, ktx: &Kontekst) -> Result<Utgangskode, AppFeil> {
    let _filter = Sokefilter::fra_args(&args.filtre, &ktx.konfig);
    let katalog = ktx.katalog()?;
    let mut lager = ktx.apne_lager()?;
    let _kilder =
        kilder::oppfrisk(&mut lager, &ktx.konfig, &katalog, ktx.hentemodus(), &[]).await?;
    super::krev_prisdata(&lager)?;

    // M2: KAMPANJE/PRISFALL, sortert etter vurdering og så literpris.
    Err(AppFeil::IkkeImplementert("tilbudsvisning, M2"))
}
