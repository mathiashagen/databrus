//! Price history: reference prices, verdicts and deal detection (SPEC §7.6–7.9).

pub mod deals;
pub mod stats;
pub mod verdict;

use std::collections::HashMap;

use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};

pub use stats::{Interval, References};
pub use verdict::{Thresholds, assess};

use crate::db::{HistoricPrice, StoredPrice};
use crate::model::{Chain, Ore, ProductId};

/// An interval whose `last_seen` is more than this before the next one started ends at
/// `last_seen`, and the time between is unknown (SPEC §7.2).
const MAX_GAP: SignedDuration = SignedDuration::from_hours(3 * 24);

/// History is kept per product and chain, across listings and sources.
pub type SeriesKey = (ProductId, Chain);

/// Reference values for every (product, chain) with history. `liter_price` gives the
/// ranked liter price of a stored price on a date, or `None` to leave it out.
pub fn references(
    history: &[HistoricPrice],
    now: Timestamp,
    liter_price: impl Fn(&StoredPrice, Date) -> Option<Ore>,
) -> HashMap<SeriesKey, References> {
    let mut intervals: HashMap<SeriesKey, Vec<Interval>> = HashMap::new();
    for listing in history.chunk_by(|a, b| a.price.listing_id == b.price.listing_id) {
        for (i, entry) in listing.iter().enumerate() {
            let price = &entry.price;
            // Sold-out and suspicious prices are not prices anyone could pay.
            if price.suspicious || price.available == Some(false) {
                continue;
            }
            let Some(liter_price) = liter_price(price, oslo_date(entry.valid_from)) else {
                continue;
            };
            let to = match listing.get(i + 1) {
                Some(next) if next.valid_from.duration_since(price.last_seen) <= MAX_GAP => {
                    next.valid_from
                }
                _ => price.last_seen,
            };
            intervals
                .entry((price.product.clone(), price.chain))
                .or_default()
                .push(Interval {
                    liter_price,
                    from: entry.valid_from,
                    to: to.max(entry.valid_from),
                });
        }
    }
    intervals
        .into_iter()
        .map(|(key, intervals)| (key, stats::compute(&stats::merge(&intervals), now)))
        .collect()
}

fn oslo() -> TimeZone {
    TimeZone::get("Europe/Oslo").unwrap_or(TimeZone::UTC)
}

/// The date in Norway at `time`. Day boundaries always follow Europe/Oslo (SPEC §7.2).
pub fn oslo_date(time: Timestamp) -> Date {
    time.to_zoned(oslo()).date()
}

/// Midnight in Norway at the start of `date`.
pub fn start_of_day(date: Date) -> Timestamp {
    date.to_zoned(oslo())
        .map(|z| z.timestamp())
        .unwrap_or_else(|_| {
            date.to_zoned(TimeZone::UTC)
                .map_or(Timestamp::MIN, |z| z.timestamp())
        })
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::*;
    use crate::model::SourceId;

    fn day(n: i64) -> Timestamp {
        start_of_day(date(2026, 10, 1)) + SignedDuration::from_hours(24 * n + 12)
    }

    fn entry(listing_id: i64, kr: i64, from: i64, last_seen: i64) -> HistoricPrice {
        HistoricPrice {
            price: StoredPrice {
                listing_id,
                source: SourceId::Kassalapp,
                chain: Chain::Meny,
                product: ProductId("monster-ultra-white-05".into()),
                pack_size: 1,
                verified: true,
                shelf_price: Ore(kr * 100),
                member_price: None,
                offer: None,
                available: Some(true),
                suspicious: false,
                last_seen: day(last_seen),
            },
            valid_from: day(from),
        }
    }

    fn refs(history: &[HistoricPrice]) -> References {
        let map = references(history, day(0), |p, _| Some(p.shelf_price));
        map[&(ProductId("monster-ultra-white-05".into()), Chain::Meny)]
    }

    #[test]
    fn an_interval_lasts_until_the_next_one_starts() {
        let refs = refs(&[entry(1, 40, -30, -21), entry(1, 30, -20, 0)]);
        assert_eq!(refs.coverage_days, 31);
        assert_eq!(refs.atl, Some(Ore(4000)));
    }

    #[test]
    fn a_long_silence_is_a_gap() {
        // Last seen on day -25, next price on day -20: five days unknown.
        let refs = refs(&[entry(1, 40, -30, -25), entry(1, 30, -20, 0)]);
        assert_eq!(refs.coverage_days, 6 + 21);
    }

    #[test]
    fn sold_out_and_suspicious_prices_are_left_out() {
        let mut sold_out = entry(1, 10, -20, -10);
        sold_out.price.available = Some(false);
        let mut suspicious = entry(1, 5, -10, -5);
        suspicious.price.suspicious = true;
        let refs = refs(&[
            entry(1, 40, -30, -20),
            sold_out,
            suspicious,
            entry(1, 30, -5, 0),
        ]);
        assert_eq!(refs.atl, Some(Ore(4000)));
    }

    #[test]
    fn listings_of_the_same_product_and_chain_share_a_series() {
        let refs = refs(&[
            entry(1, 40, -30, 0),
            entry(2, 35, -30, -10),
            entry(2, 38, -10, 0),
        ]);
        // The cheaper listing wins at each moment: 35, then 38.
        assert_eq!(refs.before_current, Some(Ore(3500)));
    }
}
