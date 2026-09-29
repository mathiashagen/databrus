//! Søkeresultatet som tabell (SPEC §3.4, §11).
//!
//! Vurdering og Trend kommer med historikken i M2; til da vises de ikke.

use comfy_table::{Attribute, Cell, CellAlignment, Color, Table};
use jiff::Timestamp;

use super::{format, tabell};
use crate::kilder::{KildeInfo, Kildestatus};
use crate::modell::{BruktPris, Resultat, Tilbudsmerke};
use crate::prising::effektiv_enhetspris;
use crate::sok::rangering::{MAKS_ALDER_TIMER, Soketreff};

/// Under denne bredden droppes Pant-kolonnen.
const SMAL: u16 = 80;
/// Rader eldre enn dette viser alderen etter kjeden og tones ned.
const GAMMEL_TIMER: u32 = 24;

/// Bygger tabellen. `bredde` er terminalbredden (None når utdata ikke går til en
/// terminal), og `farge` om celler skal styles.
pub fn tabell(treff: &Soketreff, farge: bool, bredde: Option<u16>) -> Table {
    let smal = bredde.is_some_and(|b| b < SMAL);
    let mut overskrifter = vec!["Produkt", "Str.", "Kjede", "Pris", "Kr/L"];
    if !smal {
        overskrifter.push("Pant");
    }
    overskrifter.push("Tilbud");

    let mut t = tabell::ny(&overskrifter);
    if let Some(bredde) = bredde {
        t.set_width(bredde);
    }
    if farge {
        t.enforce_styling();
    }
    for kolonne in [3, 4, 5] {
        if let Some(k) = t.column_mut(kolonne)
            && (kolonne < 5 || !smal)
        {
            k.set_cell_alignment(CellAlignment::Right);
        }
    }

    for (i, rad) in treff.rader.iter().enumerate() {
        let (tilbud, kampanje) = tilbudstekst(rad);
        let mut celler = vec![
            produktnavn(rad),
            format::liter(rad.produkt.volum_ml),
            kjedenavn(rad),
            format::kr(rad.effektiv_enhetspris_ore),
            format::kr(rad.literpris_ore),
        ];
        if !smal {
            celler.push(format!("+{}", format::kr(rad.pant_ore)));
        }
        celler.push(tilbud);

        let nedtonet = rad.alder_timer >= GAMMEL_TIMER || !rad.tilgjengelig;
        let siste = celler.len() - 1;
        let celler: Vec<Cell> = celler
            .into_iter()
            .enumerate()
            .map(|(j, tekst)| {
                let mut celle = Cell::new(tekst);
                if farge {
                    if i == 0 {
                        celle = celle.add_attribute(Attribute::Bold);
                    }
                    if nedtonet {
                        celle = celle.add_attribute(Attribute::Dim);
                    }
                    if j == siste && kampanje {
                        celle = celle.fg(Color::Yellow);
                    }
                }
                celle
            })
            .collect();
        t.add_row(celler);
    }
    t
}

/// «Viser 20 av 47 treff · priser hentet for 2 t siden · 3 eldre enn 14 dager skjult …»
pub fn bunntekst(treff: &Soketreff, kilder: &[KildeInfo], na: Timestamp) -> String {
    let mut deler = vec![format!(
        "Viser {} av {} treff",
        treff.rader.len(),
        treff.totalt
    )];
    if let Some(hentet) = kilder
        .iter()
        .filter(|k| k.status == Kildestatus::Ok)
        .filter_map(|k| k.hentet)
        .max()
    {
        let timer = u32::try_from(na.duration_since(hentet).as_hours()).unwrap_or(0);
        deler.push(format!("priser hentet for {} siden", format::alder(timer)));
    }
    let skjult = treff.skjult;
    if skjult.gamle > 0 {
        deler.push(format!(
            "{} eldre enn {} dager skjult",
            skjult.gamle,
            MAKS_ALDER_TIMER / 24
        ));
    }
    if skjult.utsolgte > 0 {
        deler.push(format!("{} utsolgte skjult", skjult.utsolgte));
    }
    if skjult.mistenkelige > 0 {
        deler.push(format!("{} mistenkelige skjult", skjult.mistenkelige));
    }
    if skjult.totalt() > 0 {
        deler.push("--alle viser alt".into());
    }
    let feilet = kilder
        .iter()
        .filter(|k| k.status == Kildestatus::Feilet)
        .count();
    match feilet {
        0 => {}
        1 => deler.push("1 kilde feilet (se over)".into()),
        n => deler.push(format!("{n} kilder feilet (se over)")),
    }
    deler.join(" · ")
}

fn produktnavn(rad: &Resultat) -> String {
    if rad.produkt.verifisert {
        rad.produkt.navn.clone()
    } else {
        format!("{} ?", rad.produkt.navn)
    }
}

fn kjedenavn(rad: &Resultat) -> String {
    if rad.alder_timer >= GAMMEL_TIMER {
        format!(
            "{} {}",
            rad.kjede.visningsnavn(),
            format::alder(rad.alder_timer)
        )
    } else {
        rad.kjede.visningsnavn().to_owned()
    }
}

/// Teksten i Tilbud-kolonnen, og om den er en kampanje (vises i gult).
fn tilbudstekst(rad: &Resultat) -> (String, bool) {
    let mut deler = Vec::new();
    let kampanje = rad.tilbudsmerke == Some(Tilbudsmerke::Kampanje);
    match rad.tilbudsmerke {
        Some(Tilbudsmerke::Kampanje) if rad.minsteantall > rad.antall_i_pakke => {
            deler.push(format!("KAMPANJE {}stk", rad.minsteantall));
        }
        Some(Tilbudsmerke::Kampanje) => deler.push("KAMPANJE".into()),
        Some(Tilbudsmerke::Prisfall) => deler.push("PRISFALL".into()),
        None => {}
    }
    match (rad.brukt_pris, rad.medlemspris_ore) {
        (BruktPris::Medlemspris, _) => deler.push("MEDLEM".into()),
        (_, Some(medlemspris)) => {
            let per_boks = effektiv_enhetspris(medlemspris, rad.antall_i_pakke, None).enhetspris;
            deler.push(format!("MEDLEM {}", format::kr(per_boks)));
        }
        _ => {}
    }
    if rad.antall_i_pakke > 1 {
        deler.push(format!("{}-pakning", rad.antall_i_pakke));
    }
    if !rad.tilgjengelig {
        deler.push("utsolgt".into());
    }
    (deler.join(" · "), kampanje)
}

#[cfg(test)]
mod tests {
    use jiff::ToSpan;
    use jiff::civil::date;

    use super::*;
    use crate::cli::SokArgs;
    use crate::katalog::Katalog;
    use crate::kilder::Kildestatus;
    use crate::konfig::Konfig;
    use crate::lager::LagretPris;
    use crate::modell::{
        KildeId, Kjede, Medlemspris, Medlemsprogram, Ore, ProduktId, Tilbud, Tilbudsinfo,
    };
    use crate::sok::Sokefilter;
    use crate::sok::rangering::ranger;

    const KATALOG: &str = r#"
        [[produkt]]
        id = "monster-ultra-white-500-boks"
        navn = "Monster Ultra White"
        merke = "monster"
        smak = "ultra-white"
        sukkerfri = true
        volum_ml = 500
        beholder = "boks"

        [[produkt]]
        id = "red-bull-energy-drink-250-boks"
        navn = "Red Bull Energy Drink"
        merke = "red-bull"
        smak = "original"
        sukkerfri = false
        volum_ml = 250
        beholder = "boks"
    "#;

    fn na() -> Timestamp {
        "2026-09-29T12:00:00Z".parse().unwrap()
    }

    fn pris(produkt: &str, kjede: Kjede, hyllepris: i64, timer_siden: i64) -> LagretPris {
        LagretPris {
            oppforing_id: 0,
            kilde: KildeId::Kassalapp,
            kjede,
            produkt: ProduktId(produkt.into()),
            antall: 1,
            verifisert: true,
            hyllepris: Ore(hyllepris),
            medlemspris: None,
            tilbud: None,
            tilgjengelig: None,
            mistenkelig: false,
            sist_sett: na() - timer_siden.hours(),
        }
    }

    fn eksempel() -> Soketreff {
        let mut kampanje = pris("monster-ultra-white-500-boks", Kjede::Kiwi, 2490, 2);
        kampanje.tilbud = Some(Tilbudsinfo {
            tilbud: Tilbud::NForSum {
                n: 3,
                sum_ore: Ore(4770),
            },
            gyldig_fra: Some(date(2026, 9, 28)),
            gyldig_til: Some(date(2026, 10, 4)),
            kilde_merket: true,
        });
        let mut medlem = pris("monster-ultra-white-500-boks", Kjede::CoopExtra, 2190, 3);
        medlem.medlemspris = Some(Medlemspris {
            pris: Ore(1890),
            program: Medlemsprogram::Coop,
        });
        let mut pakke = pris("red-bull-energy-drink-250-boks", Kjede::Joker, 9960, 72);
        pakke.antall = 4;
        let mut ukjent = pris("monster-ultra-white-500-boks", Kjede::Oda, 2240, 5);
        ukjent.verifisert = false;
        let gammel = pris(
            "red-bull-energy-drink-250-boks",
            Kjede::Rema,
            1990,
            24 * 400,
        );

        let katalog = Katalog::fra_toml(KATALOG).unwrap();
        let filter = Sokefilter::fra_args(&SokArgs::default(), &Konfig::default());
        ranger(
            &[kampanje, medlem, pakke, ukjent, gammel],
            &katalog,
            &filter,
            &Konfig::default(),
            na(),
            date(2026, 9, 29),
        )
    }

    #[test]
    fn tabell_uten_farger() {
        insta::assert_snapshot!(tabell(&eksempel(), false, Some(120)).to_string());
    }

    #[test]
    fn smal_tabell_dropper_pant() {
        let tekst = tabell(&eksempel(), false, Some(70)).to_string();
        assert!(!tekst.contains("Pant"));
        assert!(tekst.contains("Tilbud"));
    }

    #[test]
    fn bunntekst_med_skjulte_og_feilede_kilder() {
        let kilder = [
            KildeInfo {
                id: KildeId::Kassalapp,
                status: Kildestatus::Ok,
                hentet: Some(na() - 2.hours()),
                feil: None,
                nye_oppforinger: None,
                matchet: None,
            },
            KildeInfo {
                id: KildeId::Oda,
                status: Kildestatus::Feilet,
                hentet: None,
                feil: Some("HTTP 503".into()),
                nye_oppforinger: None,
                matchet: None,
            },
        ];
        assert_eq!(
            bunntekst(&eksempel(), &kilder, na()),
            "Viser 4 av 4 treff · priser hentet for 2 t siden · \
             1 eldre enn 14 dager skjult · --alle viser alt · 1 kilde feilet (se over)"
        );
    }
}
