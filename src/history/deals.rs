//! What counts as a deal (SPEC §7.8).
//!
//! KAMPANJE (the source flags an active offer) is in place. PRISFALL (≥ 10 % below the
//! 90-day median) needs the history statistics and comes in M2.

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;

use crate::model::{DealBadge, OfferInfo};

/// Today's date in Norway. Day boundaries always follow Europe/Oslo.
pub fn today_oslo() -> Date {
    let now = Timestamp::now();
    now.in_tz("Europe/Oslo")
        .unwrap_or_else(|_| now.to_zoned(TimeZone::UTC))
        .date()
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

pub fn deal_badge(offer: Option<&OfferInfo>, date: Date) -> Option<DealBadge> {
    offer
        .filter(|o| campaign_active(o, date))
        .map(|_| DealBadge::Campaign)
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

    #[test]
    fn unflagged_is_not_a_campaign() {
        let o = offer(None, None, false);
        assert_eq!(deal_badge(Some(&o), date(2026, 1, 1)), None);
    }
}
