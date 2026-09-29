//! Fornuftsgrenser (SPEC §5.5). Mistenkelige observasjoner holdes utenfor statistikk
//! og rangering, og vises bare med `--alle`.

use crate::modell::Ore;

const MIN_ENHETSPRIS: Ore = Ore(500);
const MAKS_ENHETSPRIS: Ore = Ore(15_000);
const MIN_LITERPRIS: Ore = Ore(1_000);
const MAKS_LITERPRIS: Ore = Ore(40_000);
/// Større prisendring enn dette uten et tilbud som forklarer det, er mistenkelig.
const MAKS_UFORKLART_ENDRING_PROSENT: i64 = 60;

pub fn er_mistenkelig(
    enhetspris: Ore,
    literpris: Ore,
    forrige_enhetspris: Option<Ore>,
    har_tilbud: bool,
) -> bool {
    let utenfor_grensene = !(MIN_ENHETSPRIS..=MAKS_ENHETSPRIS).contains(&enhetspris)
        || !(MIN_LITERPRIS..=MAKS_LITERPRIS).contains(&literpris);

    let uforklart_hopp = match forrige_enhetspris {
        Some(forrige) if !har_tilbud && forrige.0 > 0 => {
            (enhetspris.0 - forrige.0).abs() * 100 > forrige.0 * MAKS_UFORKLART_ENDRING_PROSENT
        }
        _ => false,
    };

    utenfor_grensene || uforklart_hopp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vanlig_pris_er_i_orden() {
        assert!(!er_mistenkelig(
            Ore(2490),
            Ore(4980),
            Some(Ore(2590)),
            false
        ));
    }

    #[test]
    fn for_billig_eller_for_dyr() {
        assert!(er_mistenkelig(Ore(290), Ore(580), None, false));
        assert!(er_mistenkelig(Ore(19_900), Ore(39_800), None, false));
        assert!(er_mistenkelig(Ore(2490), Ore(900), None, false));
    }

    #[test]
    fn stort_hopp_uten_tilbud() {
        assert!(er_mistenkelig(Ore(900), Ore(1800), Some(Ore(2490)), false));
        assert!(!er_mistenkelig(Ore(900), Ore(1800), Some(Ore(2490)), true));
    }
}
