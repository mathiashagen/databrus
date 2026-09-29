//! Fra lagrede priser til rangerte rader (SPEC §5, §3.2).

use std::collections::HashMap;

use jiff::Timestamp;
use jiff::civil::Date;

use super::{Sokefilter, Sortering};
use crate::historikk::deteksjon;
use crate::katalog::Katalog;
use crate::konfig::Konfig;
use crate::lager::LagretPris;
use crate::modell::{
    BruktPris, Medlemsprogram, Ore, Produkt, Produktsammendrag, Resultat, Tilbud, Vurdering,
    Vurderingsinfo,
};
use crate::prising::{self, Prisberegning, effektiv_enhetspris};

/// Priser eldre enn dette skjules uten `--alle` (SPEC §8).
pub const MAKS_ALDER_TIMER: u32 = 14 * 24;

/// Resultatet av et søk.
#[derive(Debug, Clone, Default)]
pub struct Soketreff {
    /// Rangerte rader, avkortet til `antall` uten `--alle`.
    pub rader: Vec<Resultat>,
    /// Antall treff før avkorting.
    pub totalt: usize,
    pub skjult: Skjult,
}

/// Rader som passet filtrene, men ble skjult uten `--alle`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Skjult {
    pub gamle: usize,
    pub utsolgte: usize,
    pub mistenkelige: usize,
}

impl Skjult {
    pub fn totalt(&self) -> usize {
        self.gamle + self.utsolgte + self.mistenkelige
    }
}

/// Filtrerer, beregner og rangerer. `na` og `idag` sendes inn, slik at resultatet er
/// forutsigbart i tester.
pub fn ranger(
    priser: &[LagretPris],
    katalog: &Katalog,
    filter: &Sokefilter,
    konfig: &Konfig,
    na: Timestamp,
    idag: Date,
) -> Soketreff {
    let mut skjult = Skjult::default();
    let mut rader = Vec::new();

    for pris in priser {
        let Some(produkt) = katalog.finn(&pris.produkt) else {
            continue;
        };
        if !filter.kjede_passer(pris.kjede) || !filter.produkt_passer(produkt) {
            continue;
        }

        let (beregning, brukt_pris) = beregn(pris, filter.enkeltvis, &konfig.medlemskap, idag);
        let literpris = prising::literpris(beregning.enhetspris, produkt.volum_ml);
        if filter.maks_pris.is_some_and(|m| beregning.enhetspris > m)
            || filter.maks_literpris.is_some_and(|m| literpris > m)
        {
            continue;
        }

        let alder_timer = u32::try_from(na.duration_since(pris.sist_sett).as_hours()).unwrap_or(0);
        let utsolgt = pris.tilgjengelig == Some(false);
        if !filter.alle {
            if alder_timer > MAKS_ALDER_TIMER {
                skjult.gamle += 1;
                continue;
            }
            if utsolgt {
                skjult.utsolgte += 1;
                continue;
            }
            if pris.mistenkelig {
                skjult.mistenkelige += 1;
                continue;
            }
        }

        rader.push(resultat(
            pris,
            produkt,
            beregning,
            brukt_pris,
            literpris,
            alder_timer,
            konfig,
            idag,
        ));
    }

    if !filter.alle {
        rader = billigste_per_produkt_og_kjede(rader);
    }
    sorter(&mut rader, filter.sortering);

    let totalt = rader.len();
    if !filter.alle {
        rader.truncate(filter.antall);
    }
    Soketreff {
        rader,
        totalt,
        skjult,
    }
}

/// Effektiv pris per beholder, og hvilken pris som ble brukt (SPEC §5.2–5.3).
fn beregn(
    pris: &LagretPris,
    enkeltvis: bool,
    medlemskap: &[Medlemsprogram],
    idag: Date,
) -> (Prisberegning, BruktPris) {
    let tilbud = pris
        .tilbud
        .as_ref()
        .filter(|t| deteksjon::kampanje_aktiv(t, idag))
        .map(|t| &t.tilbud)
        // `--enkeltvis`: bare tilbud som gjelder når man kjøper én.
        .filter(|t| !enkeltvis || matches!(t, Tilbud::Fastpris { .. } | Tilbud::Prosent { .. }));

    let mut beregning = effektiv_enhetspris(pris.hyllepris, pris.antall, tilbud);
    let mut brukt = if beregning.brukt_tilbud {
        BruktPris::Tilbud
    } else {
        BruktPris::Hyllepris
    };

    if let Some(medlem) = pris.medlemspris
        && medlemskap.contains(&medlem.program)
    {
        let som_medlem = effektiv_enhetspris(medlem.pris, pris.antall, None);
        if som_medlem.enhetspris < beregning.enhetspris {
            beregning = som_medlem;
            brukt = BruktPris::Medlemspris;
        }
    }
    (beregning, brukt)
}

#[allow(clippy::too_many_arguments)]
fn resultat(
    pris: &LagretPris,
    produkt: &Produkt,
    beregning: Prisberegning,
    brukt_pris: BruktPris,
    literpris: Ore,
    alder_timer: u32,
    konfig: &Konfig,
    idag: Date,
) -> Resultat {
    let aktivt_tilbud = pris
        .tilbud
        .clone()
        .filter(|t| deteksjon::kampanje_aktiv(t, idag));
    let pant = prising::pant(produkt.volum_ml, &konfig.pant);
    Resultat {
        produkt: Produktsammendrag {
            id: produkt.id.clone(),
            navn: produkt.navn.clone(),
            merke: produkt.merke.clone(),
            smak: produkt.smak.clone(),
            sukkerfri: produkt.sukkerfri,
            volum_ml: produkt.volum_ml,
            beholder: produkt.beholder,
            egenmerke: produkt.egenmerke,
            verifisert: pris.verifisert,
        },
        kjede: pris.kjede,
        kilde: pris.kilde,
        antall_i_pakke: pris.antall,
        hyllepris_ore: pris.hyllepris,
        medlemspris_ore: pris.medlemspris.map(|m| m.pris),
        medlemsprogram: pris.medlemspris.map(|m| m.program),
        effektiv_enhetspris_ore: beregning.enhetspris,
        literpris_ore: literpris,
        minsteantall: beregning.minsteantall,
        brukt_pris,
        pant_ore: pant,
        pant_minsteantall_ore: Ore(pant.0 * i64::from(beregning.minsteantall)),
        tilbudsmerke: deteksjon::tilbudsmerke(aktivt_tilbud.as_ref(), idag),
        tilbud: aktivt_tilbud,
        // Vurdering og referansepriser kommer med historikken i M2.
        vurdering: Vurderingsinfo {
            verdi: Vurdering::Ukjent,
            l30_ore: None,
            m90_ore: None,
            atl_ore: None,
            dekning_dager: 0,
        },
        prisspenn: None,
        tilgjengelig: pris.tilgjengelig != Some(false),
        sist_sett: pris.sist_sett,
        alder_timer,
    }
}

/// Uten `--alle` vises bare billigste oppføring per (produkt, kjede) – f.eks. enten
/// enkeltboksen eller firepakningen, ikke begge (SPEC §5.2).
fn billigste_per_produkt_og_kjede(rader: Vec<Resultat>) -> Vec<Resultat> {
    let mut beste: HashMap<_, Resultat> = HashMap::new();
    for rad in rader {
        let nokkel = (rad.produkt.id.clone(), rad.kjede);
        match beste.get(&nokkel) {
            Some(naverende)
                if (naverende.literpris_ore, naverende.minsteantall)
                    <= (rad.literpris_ore, rad.minsteantall) => {}
            _ => {
                beste.insert(nokkel, rad);
            }
        }
    }
    beste.into_values().collect()
}

fn sorter(rader: &mut [Resultat], sortering: Sortering) {
    // Stabil rekkefølge ved like priser: navn, størrelse, kjede.
    let navn = |r: &Resultat| (r.produkt.navn.to_lowercase(), r.produkt.volum_ml, r.kjede);
    match sortering {
        // Rabatt krever 90-dagersmedianen (M2); til da sorteres det som literpris.
        Sortering::Literpris | Sortering::Rabatt => {
            rader.sort_by_cached_key(|r| (r.literpris_ore, r.effektiv_enhetspris_ore, navn(r)));
        }
        Sortering::Pris => {
            rader.sort_by_cached_key(|r| (r.effektiv_enhetspris_ore, r.literpris_ore, navn(r)));
        }
        Sortering::Navn => rader.sort_by_cached_key(|r| (navn(r), r.literpris_ore)),
    }
}

#[cfg(test)]
mod tests {
    use jiff::ToSpan;
    use jiff::civil::date;

    use super::*;
    use crate::cli::{KjedeValg, SokArgs};
    use crate::modell::{KildeId, Kjede, Medlemspris, ProduktId, Tilbudsinfo};

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

    fn idag() -> Date {
        date(2026, 9, 29)
    }

    fn pris(id: i64, produkt: &str, kjede: Kjede, hyllepris: i64) -> LagretPris {
        LagretPris {
            oppforing_id: id,
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
            sist_sett: na() - 2.hours(),
        }
    }

    const WHITE: &str = "monster-ultra-white-500-boks";
    const RED_BULL: &str = "red-bull-energy-drink-250-boks";

    fn sok(priser: &[LagretPris], args: SokArgs, konfig: &Konfig) -> Soketreff {
        let katalog = Katalog::fra_toml(KATALOG).unwrap();
        let filter = Sokefilter::fra_args(&args, konfig);
        ranger(priser, &katalog, &filter, konfig, na(), idag())
    }

    fn kjeder(treff: &Soketreff) -> Vec<(Kjede, i64)> {
        treff
            .rader
            .iter()
            .map(|r| (r.kjede, r.literpris_ore.0))
            .collect()
    }

    #[test]
    fn sortert_etter_literpris_med_pant_for_seg() {
        let priser = [
            pris(1, WHITE, Kjede::Meny, 3290),
            pris(2, WHITE, Kjede::Spar, 1690),
            pris(3, RED_BULL, Kjede::Joker, 2490),
        ];
        let treff = sok(&priser, SokArgs::default(), &Konfig::default());
        // 16,90/0,5 = 33,80 · 32,90/0,5 = 65,80 · 24,90/0,25 = 99,60
        assert_eq!(
            kjeder(&treff),
            [
                (Kjede::Spar, 3380),
                (Kjede::Meny, 6580),
                (Kjede::Joker, 9960)
            ]
        );
        let spar = &treff.rader[0];
        assert_eq!(spar.pant_ore, Ore(200));
        assert_eq!(spar.effektiv_enhetspris_ore, Ore(1690));
        assert_eq!(spar.brukt_pris, BruktPris::Hyllepris);
        assert_eq!(spar.alder_timer, 2);
    }

    #[test]
    fn filtre_pa_kjede_og_literpris() {
        let priser = [
            pris(1, WHITE, Kjede::Meny, 3290),
            pris(2, WHITE, Kjede::Spar, 1690),
            pris(3, RED_BULL, Kjede::Spar, 2490),
        ];
        let args = SokArgs {
            butikk: vec![KjedeValg::En(Kjede::Spar)],
            maks_literpris: Some(Ore::fra_kr(50)),
            ..SokArgs::default()
        };
        let treff = sok(&priser, args, &Konfig::default());
        assert_eq!(kjeder(&treff), [(Kjede::Spar, 3380)]);
    }

    #[test]
    fn gamle_priser_skjules_uten_alle() {
        let mut gammel = pris(1, WHITE, Kjede::Kiwi, 1990);
        gammel.sist_sett = na() - (15 * 24).hours();
        let priser = [gammel, pris(2, WHITE, Kjede::Meny, 3290)];

        let treff = sok(&priser, SokArgs::default(), &Konfig::default());
        assert_eq!(kjeder(&treff), [(Kjede::Meny, 6580)]);
        assert_eq!(treff.skjult.gamle, 1);

        let alle = SokArgs {
            alle: true,
            ..SokArgs::default()
        };
        let treff = sok(&priser, alle, &Konfig::default());
        assert_eq!(treff.rader.len(), 2);
        assert_eq!(treff.rader[0].alder_timer, 15 * 24);
    }

    #[test]
    fn flerpakning_regnes_per_boks_og_billigste_vises() {
        let enkel = pris(1, WHITE, Kjede::Joker, 2990);
        let mut firepakning = pris(2, WHITE, Kjede::Joker, 9960);
        firepakning.antall = 4;
        let priser = [enkel, firepakning];

        let treff = sok(&priser, SokArgs::default(), &Konfig::default());
        assert_eq!(treff.rader.len(), 1);
        let rad = &treff.rader[0];
        assert_eq!(rad.effektiv_enhetspris_ore, Ore(2490));
        assert_eq!(rad.minsteantall, 4);
        assert_eq!(rad.pant_minsteantall_ore, Ore(800));

        let alle = SokArgs {
            alle: true,
            ..SokArgs::default()
        };
        assert_eq!(sok(&priser, alle, &Konfig::default()).rader.len(), 2);
    }

    #[test]
    fn aktivt_flerkjopstilbud_brukes_utenom_enkeltvis() {
        let mut tilbud = pris(1, WHITE, Kjede::Kiwi, 2490);
        tilbud.tilbud = Some(Tilbudsinfo {
            tilbud: Tilbud::NForM { n: 3, m: 2 },
            gyldig_fra: Some(date(2026, 9, 28)),
            gyldig_til: Some(date(2026, 10, 4)),
            kilde_merket: true,
        });
        let priser = [tilbud];

        let rad = &sok(&priser, SokArgs::default(), &Konfig::default()).rader[0];
        assert_eq!(rad.effektiv_enhetspris_ore, Ore(1660));
        assert_eq!(rad.minsteantall, 3);
        assert_eq!(rad.brukt_pris, BruktPris::Tilbud);
        assert_eq!(
            rad.tilbudsmerke,
            Some(crate::modell::Tilbudsmerke::Kampanje)
        );

        let enkeltvis = SokArgs {
            enkeltvis: true,
            ..SokArgs::default()
        };
        let rad = &sok(&priser, enkeltvis, &Konfig::default()).rader[0];
        assert_eq!(rad.effektiv_enhetspris_ore, Ore(2490));
        assert_eq!(rad.minsteantall, 1);
    }

    #[test]
    fn utlopt_tilbud_ignoreres() {
        let mut utlopt = pris(1, WHITE, Kjede::Kiwi, 2490);
        utlopt.tilbud = Some(Tilbudsinfo {
            tilbud: Tilbud::Fastpris {
                pris_ore: Ore(1590),
            },
            gyldig_fra: None,
            gyldig_til: Some(date(2026, 9, 27)),
            kilde_merket: true,
        });
        let rad = &sok(&[utlopt], SokArgs::default(), &Konfig::default()).rader[0];
        assert_eq!(rad.effektiv_enhetspris_ore, Ore(2490));
        assert_eq!(rad.tilbud, None);
        assert_eq!(rad.tilbudsmerke, None);
    }

    #[test]
    fn medlemspris_bare_for_medlemmer() {
        let mut coop = pris(1, WHITE, Kjede::CoopExtra, 2490);
        coop.medlemspris = Some(Medlemspris {
            pris: Ore(1990),
            program: Medlemsprogram::Coop,
        });
        let priser = [coop];

        let rad = &sok(&priser, SokArgs::default(), &Konfig::default()).rader[0];
        assert_eq!(rad.effektiv_enhetspris_ore, Ore(2490));
        assert_eq!(rad.medlemspris_ore, Some(Ore(1990)));

        let medlem = Konfig {
            medlemskap: vec![Medlemsprogram::Coop],
            ..Konfig::default()
        };
        let rad = &sok(&priser, SokArgs::default(), &medlem).rader[0];
        assert_eq!(rad.effektiv_enhetspris_ore, Ore(1990));
        assert_eq!(rad.brukt_pris, BruktPris::Medlemspris);
    }

    #[test]
    fn utsolgt_og_mistenkelig_skjules_uten_alle() {
        let mut utsolgt = pris(1, WHITE, Kjede::Oda, 2190);
        utsolgt.tilgjengelig = Some(false);
        let mut mistenkelig = pris(2, WHITE, Kjede::Meny, 290);
        mistenkelig.mistenkelig = true;
        let treff = sok(
            &[utsolgt, mistenkelig],
            SokArgs::default(),
            &Konfig::default(),
        );
        assert!(treff.rader.is_empty());
        assert_eq!(
            treff.skjult,
            Skjult {
                gamle: 0,
                utsolgte: 1,
                mistenkelige: 1
            }
        );
    }

    #[test]
    fn avkortes_til_standard_antall() {
        let priser: Vec<_> = Kjede::ALLE
            .iter()
            .enumerate()
            .map(|(i, k)| pris(i as i64, WHITE, *k, 2000 + i as i64))
            .collect();
        let konfig = Konfig {
            standard_antall: 3,
            ..Konfig::default()
        };
        let treff = sok(&priser, SokArgs::default(), &konfig);
        assert_eq!(treff.rader.len(), 3);
        assert_eq!(treff.totalt, Kjede::ALLE.len());
    }

    #[test]
    fn sortering_etter_pris_og_navn() {
        let priser = [
            pris(1, WHITE, Kjede::Meny, 2290),
            pris(2, RED_BULL, Kjede::Spar, 1990),
        ];
        let pris_forst = SokArgs {
            sorter: Sortering::Pris,
            ..SokArgs::default()
        };
        let treff = sok(&priser, pris_forst, &Konfig::default());
        assert_eq!(treff.rader[0].produkt.navn, "Red Bull Energy Drink");

        let navn = SokArgs {
            sorter: Sortering::Navn,
            ..SokArgs::default()
        };
        let treff = sok(&priser, navn, &Konfig::default());
        assert_eq!(treff.rader[0].produkt.navn, "Monster Ultra White");
    }
}
