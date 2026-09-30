//! Assessing a price against the history (SPEC §7.6).

use serde::{Deserialize, Serialize};

use super::References;
use crate::model::{Ore, Verdict};

/// Thresholds for the verdict, from `[vurdering]` in the config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Thresholds {
    #[serde(rename = "min_dekning_dager")]
    pub min_coverage_days: u32,
    #[serde(rename = "supert_under_median_prosent")]
    pub great_below_median_percent: u32,
    #[serde(rename = "bra_under_median_prosent")]
    pub good_below_median_percent: u32,
    #[serde(rename = "atl_toleranse_prosent")]
    pub atl_tolerance_percent: u32,
    #[serde(rename = "lureri_prisokning_prosent")]
    pub fake_price_increase_percent: u32,
    #[serde(rename = "prisfall_prosent")]
    pub price_drop_percent: u32,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            min_coverage_days: 14,
            great_below_median_percent: 20,
            good_below_median_percent: 10,
            atl_tolerance_percent: 2,
            fake_price_increase_percent: 5,
            price_drop_percent: 10,
        }
    }
}

/// Assesses `liter_price` against the references. `is_deal` says whether the row is
/// marked as a deal (§7.8). The rules are checked in the order of SPEC §7.6.
pub fn assess(
    liter_price: Ore,
    is_deal: bool,
    references: &References,
    thresholds: &Thresholds,
) -> Verdict {
    if references.coverage_days < thresholds.min_coverage_days {
        return Verdict::Unknown;
    }
    let refs = references;

    // The price was raised just before the "sale", and the sale is no better than L30.
    if is_deal
        && let (Some(l30), Some(before)) = (refs.l30, refs.before_current)
        && liter_price >= l30
        && at_least_percent_above(before, l30, thresholds.fake_price_increase_percent)
    {
        return Verdict::Fake;
    }

    let near_atl = refs.atl.is_some_and(|atl| {
        i128::from(liter_price.0) * 100
            <= i128::from(atl.0) * i128::from(100 + thresholds.atl_tolerance_percent)
    });
    let below = |percent| {
        refs.m90
            .is_some_and(|m90| at_least_percent_below(liter_price, m90, percent))
    };
    if near_atl || below(thresholds.great_below_median_percent) {
        Verdict::Great
    } else if below(thresholds.good_below_median_percent)
        && refs.l30.is_some_and(|l30| liter_price < l30)
    {
        Verdict::Good
    } else {
        Verdict::Fair
    }
}

/// `price ≤ reference × (100 − percent) / 100`, in exact integer arithmetic.
pub fn at_least_percent_below(price: Ore, reference: Ore, percent: u32) -> bool {
    let factor = 100 - i128::from(percent.min(100));
    i128::from(price.0) * 100 <= i128::from(reference.0) * factor
}

/// `price ≥ reference × (100 + percent) / 100`, in exact integer arithmetic.
fn at_least_percent_above(price: Ore, reference: Ore, percent: u32) -> bool {
    i128::from(price.0) * 100 >= i128::from(reference.0) * (100 + i128::from(percent))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refs(l30: i64, m90: i64, atl: i64, before: i64) -> References {
        References {
            l30: Some(Ore(l30 * 100)),
            m90: Some(Ore(m90 * 100)),
            atl: Some(Ore(atl * 100)),
            before_current: Some(Ore(before * 100)),
            coverage_days: 30,
        }
    }

    fn assess_kr(kr: i64, is_deal: bool, references: &References) -> Verdict {
        assess(Ore(kr * 100), is_deal, references, &Thresholds::default())
    }

    #[test]
    fn too_little_history_is_unknown() {
        let mut r = refs(40, 40, 30, 40);
        r.coverage_days = 13;
        assert_eq!(assess_kr(20, true, &r), Verdict::Unknown);
    }

    #[test]
    fn a_raised_price_before_the_sale_is_fake() {
        // L30 was 40, the price went up to 45, and the "sale" is back at 40.
        assert_eq!(assess_kr(40, true, &refs(40, 42, 35, 45)), Verdict::Fake);
        // Not marked as a deal: just a price.
        assert_eq!(assess_kr(40, false, &refs(40, 42, 35, 45)), Verdict::Fair);
        // A small raise (under 5 %) is not fake.
        assert_eq!(assess_kr(40, true, &refs(40, 42, 35, 41)), Verdict::Fair);
    }

    #[test]
    fn near_the_all_time_low_is_great() {
        // 2 % above ATL 35 is 35,70.
        let r = refs(40, 40, 35, 40);
        assert_eq!(
            assess(Ore(3570), false, &r, &Thresholds::default()),
            Verdict::Great
        );
        assert_eq!(
            assess(Ore(3571), false, &r, &Thresholds::default()),
            Verdict::Good
        );
    }

    #[test]
    fn twenty_percent_below_the_median_is_great() {
        assert_eq!(assess_kr(32, false, &refs(30, 40, 20, 40)), Verdict::Great);
    }

    #[test]
    fn good_needs_both_the_median_and_l30() {
        assert_eq!(assess_kr(36, true, &refs(38, 40, 20, 40)), Verdict::Good);
        // Ten percent below the median, but not below L30.
        assert_eq!(assess_kr(36, true, &refs(36, 40, 20, 36)), Verdict::Fair);
        assert_eq!(assess_kr(37, true, &refs(38, 40, 20, 40)), Verdict::Fair);
    }

    #[test]
    fn missing_references_are_fair() {
        let r = References {
            coverage_days: 30,
            ..References::default()
        };
        assert_eq!(assess_kr(40, true, &r), Verdict::Fair);
    }
}
