//! Søk (standardkommandoen).

use std::io::Write;

use owo_colors::OwoColorize;
use serde::Serialize;

use crate::Kontekst;
use crate::cli::SokArgs;
use crate::feil::{AppFeil, Utgangskode};
use crate::historikk::deteksjon;
use crate::kilder::{self, KildeInfo, Kildestatus};
use crate::modell::{self, Resultat};
use crate::sok::rangering::{self, Soketreff};
use crate::sok::{Sokefilter, Sortering, Sporring};
use crate::utdata::json::{self, Hode, Konvolutt};
use crate::utdata::{self, Utdataformat, resultater, tabell};

/// En rad eldre enn dette teller som utdatert for `--streng` (SPEC §8).
const UTDATERT_TIMER: u32 = 24;

#[derive(Serialize)]
struct SokInnhold<'a> {
    sporring: Sporring,
    resultater: &'a [Resultat],
}

pub async fn kjor(args: &SokArgs, ktx: &Kontekst) -> Result<Utgangskode, AppFeil> {
    let filter = Sokefilter::fra_args(args, &ktx.konfig);
    if filter.sortering == Sortering::Rabatt {
        anstream::eprintln!(
            "{} --sorter rabatt krever prishistorikk (kommer i M2) – sorterer etter literpris",
            "info:".cyan().bold()
        );
    }
    let katalog = ktx.katalog()?;
    let mut lager = ktx.apne_lager()?;
    let kilder = kilder::oppfrisk(&mut lager, &ktx.konfig, &katalog, ktx.hentemodus(), &[]).await?;
    super::krev_prisdata(&lager)?;

    let na = modell::na();
    let treff = rangering::ranger(
        &lager.siste_priser()?,
        &katalog,
        &filter,
        &ktx.konfig,
        na,
        deteksjon::idag_oslo(),
    );

    match ktx.utdataformat() {
        Utdataformat::Json => json::skriv(&Konvolutt {
            hode: Hode::ny().med_kilder(kilder.clone()),
            innhold: SokInnhold {
                sporring: filter.sporring(&args.sok),
                resultater: &treff.rader,
            },
        })?,
        Utdataformat::JsonLinjer => {
            json::skriv_linjer(&Hode::ny().med_kilder(kilder.clone()), &treff.rader)?;
        }
        Utdataformat::Tabell if !treff.rader.is_empty() => skriv_tabell(&treff, &kilder, na)?,
        Utdataformat::Tabell => {}
    }
    if treff.rader.is_empty() {
        ingen_treff(&treff);
    }

    Ok(utgangskode(ktx.globale.streng, &treff, &kilder))
}

fn skriv_tabell(
    treff: &Soketreff,
    kilder: &[KildeInfo],
    na: jiff::Timestamp,
) -> Result<(), AppFeil> {
    let farge = utdata::farger_pa();
    let t = resultater::tabell(treff, farge, tabell::terminalbredde());
    let mut ut = anstream::stdout();
    writeln!(ut, "{t}")?;
    let bunn = resultater::bunntekst(treff, kilder, na);
    if farge {
        writeln!(ut, "{}", bunn.dimmed())?;
    } else {
        writeln!(ut, "{bunn}")?;
    }
    Ok(())
}

fn ingen_treff(treff: &Soketreff) {
    let skjult = treff.skjult.totalt();
    if skjult > 0 {
        eprintln!(
            "ingen treff – {skjult} er skjult fordi de er gamle, utsolgte eller mistenkelige (--alle viser dem)"
        );
    } else {
        eprintln!("ingen treff");
    }
}

/// `--streng` gir kode 2 når en kilde feilet eller en viste rad er utdatert.
fn utgangskode(streng: bool, treff: &Soketreff, kilder: &[KildeInfo]) -> Utgangskode {
    let feilet = kilder.iter().any(|k| k.status == Kildestatus::Feilet);
    let utdatert = treff.rader.iter().any(|r| r.alder_timer > UTDATERT_TIMER);
    if streng && (feilet || utdatert) {
        Utgangskode::Streng
    } else {
        Utgangskode::Ok
    }
}
