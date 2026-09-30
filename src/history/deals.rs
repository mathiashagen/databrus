//! What counts as a deal (SPEC §7.8).
//!
//! KAMPANJE: the source flags an active offer. PRISFALL: no offer, but the price is
//! clearly below the 90-day median.

use jiff::Timestamp;
use jiff::civil::Date;

use super::verdict::{Thresholds, at_least_percent_below};
use super::{References, oslo_date};
use crate::model::{DealBadge, OfferInfo, Ore};

/// Today's date in Norway. Day boundaries always follow Europe/Oslo.
pub fn today_oslo() -> Date {
    oslo_date(Timestamp::now())
}

/// Whether a source-flagged offer applies on `date` (both ends inclusive).
pub fn campaign_active(offer: &OfferInfo, date: Date) -> bool {
    offer.source_flagged
        && offer.valid_from.is_none_or(|from| from <= date)
        && offer.valid_to.is_none_or(|to| date <= to)
}

/// Whether a source-flagged offer starts after `date` (for `tilbud --kommende`).
pub fn campaign_upcoming(offer: &OfferInfo, date: Date) -> bool {
    offer.source_flagged && offer.valid_from.is_some_and(|from| from > date)
}

/// Whether a row is a deal, and which kind. PRISFALL needs enough history to trust the
/// median.
pub fn deal_badge(
    offer: Option<&OfferInfo>,
    date: Date,
    liter_price: Ore,
    references: &References,
    thresholds: &Thresholds,
) -> Option<DealBadge> {
    if offer.is_some_and(|o| campaign_active(o, date)) {
        return Some(DealBadge::Campaign);
    }
    let covered = references.coverage_days >= thresholds.min_coverage_days;
    let dropped = references
        .m90
        .is_some_and(|m90| at_least_percent_below(liter_price, m90, thresholds.price_drop_percent));
    (covered && dropped).then_some(DealBadge::PriceDrop)
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::*;
    use crate::model::{Offer, Ore};

    fn offer(from: Option<Date>, to: Option<Date>, source_flagged: bool) -> OfferInfo {
        OfferInfo {
            offer: Offer::FixedPrice { price: Ore(1590) },
            valid_from: from,
            valid_to: to,
            source_flagged,
        }
    }

    #[test]
    fn validity_is_inclusive() {
        let o = offer(Some(date(2026, 9, 28)), Some(date(2026, 10, 4)), true);
        assert!(!campaign_active(&o, date(2026, 9, 27)));
        assert!(campaign_active(&o, date(2026, 9, 28)));
        assert!(campaign_active(&o, date(2026, 10, 4)));
        assert!(!campaign_active(&o, date(2026, 10, 5)));
        assert!(campaign_upcoming(&o, date(2026, 9, 27)));
    }

    #[test]
    fn open_ends_always_apply() {
        assert!(campaign_active(&offer(None, None, true), date(2026, 1, 1)));
    }

    fn badge(
        offer: Option<&OfferInfo>,
        kr: i64,
        m90: i64,
        coverage_days: u32,
    ) -> Option<DealBadge> {
        let references = References {
            m90: Some(Ore(m90 * 100)),
            coverage_days,
            ..References::default()
        };
        deal_badge(
            offer,
            date(2026, 1, 1),
            Ore(kr * 100),
            &references,
            &Thresholds::default(),
        )
    }

    #[test]
    fn unflagged_is_not_a_campaign() {
        let o = offer(None, None, false);
        assert_eq!(badge(Some(&o), 40, 40, 30), None);
    }

    #[test]
    fn a_campaign_needs_no_history() {
        let o = offer(None, None, true);
        assert_eq!(badge(Some(&o), 40, 40, 0), Some(DealBadge::Campaign));
    }

    #[test]
    fn price_drop_is_ten_percent_below_the_median() {
        assert_eq!(badge(None, 36, 40, 14), Some(DealBadge::PriceDrop));
        assert_eq!(badge(None, 37, 40, 14), None);
    }

    #[test]
    fn price_drop_needs_coverage() {
        assert_eq!(badge(None, 30, 40, 13), None);
    }
}
