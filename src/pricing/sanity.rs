//! Sanity bounds (SPEC §5.5). Suspicious observations are kept out of statistics and
//! ranking, and are only shown with `--alle`.

use crate::model::Ore;

const MIN_UNIT_PRICE: Ore = Ore(500);
const MAX_UNIT_PRICE: Ore = Ore(15_000);
const MIN_LITER_PRICE: Ore = Ore(1_000);
const MAX_LITER_PRICE: Ore = Ore(40_000);
/// A larger price change than this without an offer to explain it is suspicious.
const MAX_UNEXPLAINED_CHANGE_PERCENT: i64 = 60;

pub fn is_suspicious(
    unit_price: Ore,
    liter_price: Ore,
    previous_unit_price: Option<Ore>,
    has_offer: bool,
) -> bool {
    let out_of_bounds = !(MIN_UNIT_PRICE..=MAX_UNIT_PRICE).contains(&unit_price)
        || !(MIN_LITER_PRICE..=MAX_LITER_PRICE).contains(&liter_price);

    let unexplained_jump = match previous_unit_price {
        Some(previous) if !has_offer && previous.0 > 0 => {
            (unit_price.0 - previous.0).abs() * 100 > previous.0 * MAX_UNEXPLAINED_CHANGE_PERCENT
        }
        _ => false,
    };

    out_of_bounds || unexplained_jump
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_price_is_fine() {
        assert!(!is_suspicious(Ore(2490), Ore(4980), Some(Ore(2590)), false));
    }

    #[test]
    fn too_cheap_or_too_expensive() {
        assert!(is_suspicious(Ore(290), Ore(580), None, false));
        assert!(is_suspicious(Ore(19_900), Ore(39_800), None, false));
        assert!(is_suspicious(Ore(2490), Ore(900), None, false));
    }

    #[test]
    fn large_jump_without_offer() {
        assert!(is_suspicious(Ore(900), Ore(1800), Some(Ore(2490)), false));
        assert!(!is_suspicious(Ore(900), Ore(1800), Some(Ore(2490)), true));
    }
}
