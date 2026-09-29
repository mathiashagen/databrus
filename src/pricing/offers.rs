//! Effective price per container from offers and pack size (SPEC §5.2).

use crate::model::{Offer, Ore};

use super::div_round;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PriceCalc {
    /// Price per container when the offer is used.
    pub unit_price: Ore,
    /// Containers that must be bought to get `unit_price`.
    pub min_quantity: u32,
    pub offer_applied: bool,
}

impl Offer {
    /// An offer with nonsensical parameters (e.g. "3 for 5") is ignored.
    pub fn is_valid(&self) -> bool {
        match *self {
            Offer::FixedPrice { price } => price.0 > 0,
            Offer::NForM { n, m } => m > 0 && n > m,
            Offer::NForSum { n, sum } => n >= 1 && sum.0 > 0,
            Offer::Percent { percent } => percent > 0 && percent < 100,
            Offer::NthItem {
                n,
                discount_percent,
            } => n >= 2 && discount_percent > 0 && discount_percent <= 100,
        }
    }
}

/// Computes the effective price per container.
///
/// `shelf_price` is for the sellable unit, which holds `pack_size` containers. An offer is
/// only used when it actually gives a lower price; otherwise the shelf price applies.
pub fn effective_unit_price(shelf_price: Ore, pack_size: u32, offer: Option<&Offer>) -> PriceCalc {
    let pack_size = pack_size.max(1);
    let price = i128::from(shelf_price.0);
    let without_offer = PriceCalc {
        unit_price: Ore(div_round(price, i128::from(pack_size))),
        min_quantity: pack_size,
        offer_applied: false,
    };
    let Some(offer) = offer.filter(|o| o.is_valid()) else {
        return without_offer;
    };

    // (numerator, denominator, sellable units to buy) for the price per sellable unit.
    let (numerator, denominator, units): (i128, i128, u32) = match *offer {
        Offer::FixedPrice { price } => (i128::from(price.0), 1, 1),
        Offer::NForM { n, m } => (price * i128::from(m), i128::from(n), n),
        Offer::NForSum { n, sum } => (i128::from(sum.0), i128::from(n), n),
        Offer::Percent { percent } => (price * i128::from(100 - percent), 100, 1),
        Offer::NthItem {
            n,
            discount_percent,
        } => (
            price * (100 * i128::from(n) - i128::from(discount_percent)),
            100 * i128::from(n),
            n,
        ),
    };
    let unit_price = Ore(div_round(numerator, denominator * i128::from(pack_size)));

    if unit_price < without_offer.unit_price {
        PriceCalc {
            unit_price,
            min_quantity: units.saturating_mul(pack_size),
            offer_applied: true,
        }
    } else {
        without_offer
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn calc(shelf_price: i64, pack_size: u32, offer: Offer) -> (i64, u32) {
        let c = effective_unit_price(Ore(shelf_price), pack_size, Some(&offer));
        (c.unit_price.0, c.min_quantity)
    }

    #[test]
    fn without_offer() {
        let c = effective_unit_price(Ore(2490), 1, None);
        assert_eq!(
            (c.unit_price, c.min_quantity, c.offer_applied),
            (Ore(2490), 1, false)
        );
    }

    #[test]
    fn fixed_price() {
        assert_eq!(
            calc(2490, 1, Offer::FixedPrice { price: Ore(1590) }),
            (1590, 1)
        );
    }

    #[test]
    fn three_for_two() {
        // 24,90 × 2 / 3 = 16,60
        assert_eq!(calc(2490, 1, Offer::NForM { n: 3, m: 2 }), (1660, 3));
    }

    #[test]
    fn two_for_fifty() {
        let offer = Offer::NForSum {
            n: 2,
            sum: Ore::from_kr(50),
        };
        assert_eq!(calc(2990, 1, offer), (2500, 2));
    }

    #[test]
    fn percent() {
        // 24,90 × 0,7 = 17,43
        assert_eq!(calc(2490, 1, Offer::Percent { percent: 30 }), (1743, 1));
    }

    #[test]
    fn multipack_without_offer() {
        // 69,90 / 4 = 17,475 → 17,48
        let c = effective_unit_price(Ore(6990), 4, None);
        assert_eq!((c.unit_price, c.min_quantity), (Ore(1748), 4));
    }

    #[test]
    fn multipack_with_offer() {
        // Two 4-packs for 100 kr: 12,50 per can, at least 8 cans.
        let offer = Offer::NForSum {
            n: 2,
            sum: Ore::from_kr(100),
        };
        assert_eq!(calc(6990, 4, offer), (1250, 8));
    }

    #[test]
    fn third_item_free() {
        let offer = Offer::NthItem {
            n: 3,
            discount_percent: 100,
        };
        assert_eq!(calc(2490, 1, offer), (1660, 3));
    }

    #[test]
    fn second_item_half_price() {
        // 24,90 × 1,5 / 2 = 18,675 → 18,68
        let offer = Offer::NthItem {
            n: 2,
            discount_percent: 50,
        };
        assert_eq!(calc(2490, 1, offer), (1868, 2));
    }

    #[test]
    fn more_expensive_offer_is_ignored() {
        let c = effective_unit_price(Ore(1990), 1, Some(&Offer::FixedPrice { price: Ore(2490) }));
        assert_eq!((c.unit_price, c.offer_applied), (Ore(1990), false));
    }

    #[test]
    fn invalid_offer_is_ignored() {
        let c = effective_unit_price(Ore(1990), 1, Some(&Offer::NForM { n: 3, m: 5 }));
        assert_eq!((c.unit_price, c.offer_applied), (Ore(1990), false));
    }

    fn offer_strategy() -> impl Strategy<Value = Offer> {
        prop_oneof![
            (1i64..100_000).prop_map(|p| Offer::FixedPrice { price: Ore(p) }),
            (2u32..10)
                .prop_flat_map(|n| (Just(n), 1..n))
                .prop_map(|(n, m)| Offer::NForM { n, m }),
            (1u32..10, 1i64..100_000).prop_map(|(n, s)| Offer::NForSum { n, sum: Ore(s) }),
            (1u32..100).prop_map(|percent| Offer::Percent { percent }),
            (2u32..6, 1u32..=100).prop_map(|(n, d)| Offer::NthItem {
                n,
                discount_percent: d
            }),
        ]
    }

    proptest! {
        #[test]
        fn an_offer_never_raises_the_price(
            shelf_price in 1i64..100_000,
            pack_size in 1u32..24,
            offer in offer_strategy(),
        ) {
            let without = effective_unit_price(Ore(shelf_price), pack_size, None);
            let with = effective_unit_price(Ore(shelf_price), pack_size, Some(&offer));
            prop_assert!(with.unit_price <= without.unit_price);
            prop_assert!(with.min_quantity >= pack_size);
            prop_assert!(with.unit_price.0 >= 0);
        }
    }
}
