//! databrus – finn de billigste energidrikkene i Norge.
//!
//! All logikk bor i biblioteket; `main.rs` tolker bare argumenter og oversetter
//! resultatet til en utgangskode. Se `SPEC.md` for spesifikasjonen.

pub mod cli;
pub mod feil;
pub mod historikk;
pub mod katalog;
pub mod kilder;
pub mod kommando;
pub mod konfig;
pub mod lager;
pub mod modell;
pub mod overvak;
pub mod planlegg;
pub mod prising;
pub mod sok;
pub mod utdata;

use cli::{Cli, Globale, Kommando};
use feil::{AppFeil, Utgangskode};
use katalog::Katalog;
use kilder::Hentemodus;
use konfig::{Fargevalg, Konfig, Stier};
use lager::Lager;
use utdata::Utdataformat;

/// Alt en kommando trenger for å kjøre.
#[derive(Debug)]
pub struct Kontekst {
    pub globale: Globale,
    pub stier: Stier,
    pub konfig: Konfig,
}

impl Kontekst {
    pub fn utdataformat(&self) -> Utdataformat {
        if self.globale.json {
            Utdataformat::Json
        } else if self.globale.json_linjer {
            Utdataformat::JsonLinjer
        } else {
            Utdataformat::Tabell
        }
    }

    pub fn hentemodus(&self) -> Hentemodus {
        Hentemodus {
            tving: self.globale.oppdater,
            frakoblet: self.globale.frakoblet,
            stille: false,
        }
    }

    pub fn apne_lager(&self) -> Result<Lager, AppFeil> {
        Lager::apne(&self.stier.database())
    }

    pub fn katalog(&self) -> Result<Katalog, AppFeil> {
        Katalog::last(&self.stier.katalog_overstyring())
    }
}

/// Kjører en ferdig tolket kommandolinje.
pub async fn kjor(cli: Cli) -> Result<Utgangskode, AppFeil> {
    let Cli {
        globale,
        sok,
        kommando: valgt,
    } = cli;
    let stier = Stier::finn(globale.konfig.as_deref())?;

    // Disse skal virke selv når konfigurasjonsfilen er ugyldig.
    let valgt = match valgt {
        Some(Kommando::Fullforing { skall }) => {
            cli::fullforing::skriv(skall)?;
            return Ok(Utgangskode::Ok);
        }
        Some(Kommando::Konfig(k)) => return kommando::konfig::kjor(&k, &stier, &globale),
        annen => annen,
    };

    let konfig = Konfig::last(&stier.konfigfil)?;
    sett_farge(globale.farge.unwrap_or(konfig.farge));
    let ktx = Kontekst {
        globale,
        stier,
        konfig,
    };

    match valgt {
        None => kommando::sok::kjor(&sok, &ktx).await,
        Some(Kommando::Sok(args)) => kommando::sok::kjor(&args, &ktx).await,
        Some(Kommando::Tilbud(args)) => kommando::tilbud::kjor(&args, &ktx).await,
        Some(Kommando::Historikk(args)) => kommando::historikk::kjor(&args, &ktx),
        Some(Kommando::Oppdater(args)) => kommando::oppdater::kjor(&args, &ktx).await,
        Some(Kommando::Butikker) => kommando::butikker::kjor(&ktx),
        Some(Kommando::Produkter(args)) => kommando::produkter::kjor(&args, &ktx),
        Some(Kommando::Overvak(k)) => kommando::overvak::kjor(&k, &ktx),
        Some(Kommando::Planlegg(k)) => kommando::planlegg::kjor(&k, &ktx),
        Some(Kommando::Eksporter(args)) => kommando::eksporter::kjor(&args, &ktx),
        // Håndtert før konfigurasjonen ble lastet.
        Some(Kommando::Fullforing { .. } | Kommando::Konfig(_)) => Ok(Utgangskode::Ok),
    }
}

fn sett_farge(valg: Fargevalg) {
    let valg = match valg {
        Fargevalg::Auto => anstream::ColorChoice::Auto,
        Fargevalg::Alltid => anstream::ColorChoice::Always,
        Fargevalg::Aldri => anstream::ColorChoice::Never,
    };
    valg.write_global();
}
