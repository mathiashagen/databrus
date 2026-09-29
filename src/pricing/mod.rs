//! The pricing model (SPEC §5). Everything is computed in integer øre and ml; rounding
//! happens once, half up, to whole øre.

pub mod deposit;
pub mod offers;
pub mod sanity;

pub use deposit::{DepositConfig, deposit};
pub use offers::{PriceCalc, effective_unit_price};

use crate::model::{Ml, Ore};

/// Price per liter without deposit: `unit_price × 1000 / volume`, rounded half up.
pub fn liter_price(unit_price: Ore, volume: Ml) -> Ore {
    Ore(div_round(
        i128::from(unit_price.0) * 1000,
        i128::from(volume.get()),
    ))
}

/// Integer division rounded half up (away from zero for negative numbers).
/// `denominator` must be positive.
pub(crate) fn div_round(numerator: i128, denominator: i128) -> i64 {
    debug_assert!(denominator > 0);
    let quotient = if numerator >= 0 {
        (2 * numerator + denominator) / (2 * denominator)
    } else {
        -((-2 * numerator + denominator) / (2 * denominator))
    };
    i64::try_from(quotient).unwrap_or(if quotient > 0 { i64::MAX } else { i64::MIN })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ml(v: u32) -> Ml {
        Ml::new(v).unwrap()
    }

    #[test]
    fn liter_price_example_from_spec() {
        assert_eq!(liter_price(Ore(1590), ml(500)), Ore(3180));
    }

    #[test]
    fn liter_price_rounds_half_up() {
        // 24,90 / 0,33 l = 75,4545… kr/l
        assert_eq!(liter_price(Ore(2490), ml(330)), Ore(7545));
        // 1 øre / 0,4 l = 2,5 øre/l → 3
        assert_eq!(liter_price(Ore(1), ml(400)), Ore(3));
    }

    #[test]
    fn division_rounds_half_up() {
        assert_eq!(div_round(5, 2), 3);
        assert_eq!(div_round(4, 3), 1);
        assert_eq!(div_round(-5, 2), -3);
        assert_eq!(div_round(0, 7), 0);
    }
}
