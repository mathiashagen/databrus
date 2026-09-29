//! `databrus oppdater` – henter fra alle (eller valgte) kilder nå.

use owo_colors::OwoColorize;

use crate::Kontekst;
use crate::cli::OppdaterArgs;
use crate::feil::{AppFeil, Utgangskode};
use crate::kilder::{self, Hentemodus, Kildestatus};
use crate::utdata::Utdataformat;
use crate::utdata::json::{self, Hode, Konvolutt};

pub async fn kjor(args: &OppdaterArgs, ktx: &Kontekst) -> Result<Utgangskode, AppFeil> {
    if ktx.globale.frakoblet {
        return Err(AppFeil::Bruk("kan ikke oppdatere med --frakoblet".into()));
    }
    let modus = Hentemodus {
        tving: true,
        frakoblet: false,
        stille: args.stille,
    };
    let katalog = ktx.katalog()?;
    let mut lager = ktx.apne_lager()?;
    let kilder = kilder::oppfrisk(&mut lager, &ktx.konfig, &katalog, modus, &args.kilde).await?;

    let hentet: Vec<_> = kilder
        .iter()
        .filter(|k| k.nye_oppforinger.is_some())
        .collect();
    let feilet = kilder
        .iter()
        .filter(|k| k.status == Kildestatus::Feilet)
        .count();

    match ktx.utdataformat() {
        Utdataformat::Json => json::skriv(&Konvolutt::ny(Hode::ny().med_kilder(kilder.clone())))?,
        Utdataformat::JsonLinjer => json::skriv_linjer(&Hode::ny(), &kilder)?,
        Utdataformat::Tabell if !args.stille => {
            for kilde in &hentet {
                anstream::println!(
                    "{} {}: {} oppføringer, {} matchet katalogen",
                    "✓".green(),
                    kilde.id.visningsnavn(),
                    kilde.nye_oppforinger.unwrap_or(0),
                    kilde.matchet.unwrap_or(0)
                );
            }
        }
        Utdataformat::Tabell => {}
    }

    if hentet.is_empty() && feilet > 0 {
        return Err(AppFeil::AlleKilderFeilet);
    }
    if ktx.globale.streng && feilet > 0 {
        return Ok(Utgangskode::Streng);
    }
    Ok(Utgangskode::Ok)
}
