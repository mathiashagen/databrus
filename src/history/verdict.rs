//! Assessing a price against the history (SPEC §7.6). The rules come in M2.

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

/// Assesses `liter_price` against the references. Until M2, everything is `UKJENT`.
pub fn assess(
    _liter_price: Ore,
    _is_offer: bool,
    _price_before_offer: Option<Ore>,
    _references: &References,
    _thresholds: &Thresholds,
) -> Verdict {
    Verdict::Unknown
}
