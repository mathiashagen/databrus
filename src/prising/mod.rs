//! Prismodellen (SPEC §5). Alt regnes i heltall øre og ml; avrunding skjer én gang,
//! halvt opp, til hele øre.

pub mod fornuft;
pub mod pant;
pub mod tilbud;

pub use pant::{PantKonfig, pant};
pub use tilbud::{Prisberegning, effektiv_enhetspris};

use crate::modell::{Ml, Ore};

/// Literpris uten pant: `enhetspris × 1000 / volum`, avrundet halvt opp.
pub fn literpris(enhetspris: Ore, volum: Ml) -> Ore {
    Ore(del_avrundet(
        i128::from(enhetspris.0) * 1000,
        i128::from(volum.get()),
    ))
}

/// Heltallsdivisjon avrundet halvt opp (bort fra null for negative tall).
/// `nevner` må være positiv.
pub(crate) fn del_avrundet(teller: i128, nevner: i128) -> i64 {
    debug_assert!(nevner > 0);
    let kvotient = if teller >= 0 {
        (2 * teller + nevner) / (2 * nevner)
    } else {
        -((-2 * teller + nevner) / (2 * nevner))
    };
    i64::try_from(kvotient).unwrap_or(if kvotient > 0 { i64::MAX } else { i64::MIN })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ml(v: u32) -> Ml {
        Ml::new(v).unwrap()
    }

    #[test]
    fn literpris_eksempel_fra_spec() {
        assert_eq!(literpris(Ore(1590), ml(500)), Ore(3180));
    }

    #[test]
    fn literpris_avrundes_halvt_opp() {
        // 24,90 / 0,33 l = 75,4545… kr/l
        assert_eq!(literpris(Ore(2490), ml(330)), Ore(7545));
        // 1 øre / 0,4 l = 2,5 øre/l → 3
        assert_eq!(literpris(Ore(1), ml(400)), Ore(3));
    }

    #[test]
    fn deling_avrunder_halvt_opp() {
        assert_eq!(del_avrundet(5, 2), 3);
        assert_eq!(del_avrundet(4, 3), 1);
        assert_eq!(del_avrundet(-5, 2), -3);
        assert_eq!(del_avrundet(0, 7), 0);
    }
}
