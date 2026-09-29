//! Effektiv pris per beholder ut fra tilbud og pakningsstørrelse (SPEC §5.2).

use crate::modell::{Ore, Tilbud};

use super::del_avrundet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prisberegning {
    /// Pris per beholder når tilbudet utnyttes.
    pub enhetspris: Ore,
    /// Antall beholdere som må kjøpes for å få `enhetspris`.
    pub minsteantall: u32,
    pub brukt_tilbud: bool,
}

impl Tilbud {
    /// Et tilbud med meningsløse parametre (f.eks. «3 for 5») ignoreres.
    pub fn er_gyldig(&self) -> bool {
        match *self {
            Tilbud::Fastpris { pris_ore } => pris_ore.0 > 0,
            Tilbud::NForM { n, m } => m > 0 && n > m,
            Tilbud::NForSum { n, sum_ore } => n >= 1 && sum_ore.0 > 0,
            Tilbud::Prosent { prosent } => prosent > 0 && prosent < 100,
            Tilbud::NteVare { n, rabatt_prosent } => {
                n >= 2 && rabatt_prosent > 0 && rabatt_prosent <= 100
            }
        }
    }
}

/// Beregner effektiv pris per beholder.
///
/// `hyllepris` gjelder den salgbare enheten, som inneholder `antall_i_pakke` beholdere.
/// Et tilbud brukes bare når det faktisk gir lavere pris; ellers gjelder hylleprisen.
pub fn effektiv_enhetspris(
    hyllepris: Ore,
    antall_i_pakke: u32,
    tilbud: Option<&Tilbud>,
) -> Prisberegning {
    let antall = antall_i_pakke.max(1);
    let pris = i128::from(hyllepris.0);
    let uten_tilbud = Prisberegning {
        enhetspris: Ore(del_avrundet(pris, i128::from(antall))),
        minsteantall: antall,
        brukt_tilbud: false,
    };
    let Some(tilbud) = tilbud.filter(|t| t.er_gyldig()) else {
        return uten_tilbud;
    };

    // (teller, nevner, antall salgbare enheter som må kjøpes) for prisen per salgbar enhet.
    let (teller, nevner, enheter): (i128, i128, u32) = match *tilbud {
        Tilbud::Fastpris { pris_ore } => (i128::from(pris_ore.0), 1, 1),
        Tilbud::NForM { n, m } => (pris * i128::from(m), i128::from(n), n),
        Tilbud::NForSum { n, sum_ore } => (i128::from(sum_ore.0), i128::from(n), n),
        Tilbud::Prosent { prosent } => (pris * i128::from(100 - prosent), 100, 1),
        Tilbud::NteVare { n, rabatt_prosent } => (
            pris * (100 * i128::from(n) - i128::from(rabatt_prosent)),
            100 * i128::from(n),
            n,
        ),
    };
    let enhetspris = Ore(del_avrundet(teller, nevner * i128::from(antall)));

    if enhetspris < uten_tilbud.enhetspris {
        Prisberegning {
            enhetspris,
            minsteantall: enheter.saturating_mul(antall),
            brukt_tilbud: true,
        }
    } else {
        uten_tilbud
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn beregn(hyllepris: i64, antall: u32, tilbud: Tilbud) -> (i64, u32) {
        let b = effektiv_enhetspris(Ore(hyllepris), antall, Some(&tilbud));
        (b.enhetspris.0, b.minsteantall)
    }

    #[test]
    fn uten_tilbud() {
        let b = effektiv_enhetspris(Ore(2490), 1, None);
        assert_eq!(
            (b.enhetspris, b.minsteantall, b.brukt_tilbud),
            (Ore(2490), 1, false)
        );
    }

    #[test]
    fn fastpris() {
        assert_eq!(
            beregn(
                2490,
                1,
                Tilbud::Fastpris {
                    pris_ore: Ore(1590)
                }
            ),
            (1590, 1)
        );
    }

    #[test]
    fn tre_for_to() {
        // 24,90 × 2 / 3 = 16,60
        assert_eq!(beregn(2490, 1, Tilbud::NForM { n: 3, m: 2 }), (1660, 3));
    }

    #[test]
    fn to_for_femti() {
        let tilbud = Tilbud::NForSum {
            n: 2,
            sum_ore: Ore::fra_kr(50),
        };
        assert_eq!(beregn(2990, 1, tilbud), (2500, 2));
    }

    #[test]
    fn prosent() {
        // 24,90 × 0,7 = 17,43
        assert_eq!(beregn(2490, 1, Tilbud::Prosent { prosent: 30 }), (1743, 1));
    }

    #[test]
    fn flerpakning_uten_tilbud() {
        // 69,90 / 4 = 17,475 → 17,48
        let b = effektiv_enhetspris(Ore(6990), 4, None);
        assert_eq!((b.enhetspris, b.minsteantall), (Ore(1748), 4));
    }

    #[test]
    fn flerpakning_med_tilbud() {
        // To firepakninger for 100 kr: 12,50 per boks, minst 8 bokser.
        let tilbud = Tilbud::NForSum {
            n: 2,
            sum_ore: Ore::fra_kr(100),
        };
        assert_eq!(beregn(6990, 4, tilbud), (1250, 8));
    }

    #[test]
    fn tredje_gratis() {
        let tilbud = Tilbud::NteVare {
            n: 3,
            rabatt_prosent: 100,
        };
        assert_eq!(beregn(2490, 1, tilbud), (1660, 3));
    }

    #[test]
    fn andre_til_halv_pris() {
        // 24,90 × 1,5 / 2 = 18,675 → 18,68
        let tilbud = Tilbud::NteVare {
            n: 2,
            rabatt_prosent: 50,
        };
        assert_eq!(beregn(2490, 1, tilbud), (1868, 2));
    }

    #[test]
    fn dyrere_tilbud_ignoreres() {
        let b = effektiv_enhetspris(
            Ore(1990),
            1,
            Some(&Tilbud::Fastpris {
                pris_ore: Ore(2490),
            }),
        );
        assert_eq!((b.enhetspris, b.brukt_tilbud), (Ore(1990), false));
    }

    #[test]
    fn ugyldig_tilbud_ignoreres() {
        let b = effektiv_enhetspris(Ore(1990), 1, Some(&Tilbud::NForM { n: 3, m: 5 }));
        assert_eq!((b.enhetspris, b.brukt_tilbud), (Ore(1990), false));
    }

    fn tilbud_strategi() -> impl Strategy<Value = Tilbud> {
        prop_oneof![
            (1i64..100_000).prop_map(|p| Tilbud::Fastpris { pris_ore: Ore(p) }),
            (2u32..10)
                .prop_flat_map(|n| (Just(n), 1..n))
                .prop_map(|(n, m)| Tilbud::NForM { n, m }),
            (1u32..10, 1i64..100_000).prop_map(|(n, s)| Tilbud::NForSum { n, sum_ore: Ore(s) }),
            (1u32..100).prop_map(|prosent| Tilbud::Prosent { prosent }),
            (2u32..6, 1u32..=100).prop_map(|(n, r)| Tilbud::NteVare {
                n,
                rabatt_prosent: r
            }),
        ]
    }

    proptest! {
        #[test]
        fn tilbud_gir_aldri_hoyere_pris(
            hyllepris in 1i64..100_000,
            antall in 1u32..24,
            tilbud in tilbud_strategi(),
        ) {
            let uten = effektiv_enhetspris(Ore(hyllepris), antall, None);
            let med = effektiv_enhetspris(Ore(hyllepris), antall, Some(&tilbud));
            prop_assert!(med.enhetspris <= uten.enhetspris);
            prop_assert!(med.minsteantall >= antall);
            prop_assert!(med.enhetspris.0 >= 0);
        }
    }
}
