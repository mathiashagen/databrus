//! Price history: reference prices, verdicts and deal detection (SPEC §7.6–7.9).

pub mod deals;
pub mod stats;
pub mod trend;
pub mod verdict;

use std::collections::HashMap;

use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp, Zoned};

pub use stats::{Interval, References};
pub use verdict::{Thresholds, assess};

use crate::db::{HistoricPrice, StoredPrice};
use crate::model::{Chain, Ore, ProductId};

/// An interval whose `last_seen` is more than this before the next one started ends at
/// `last_seen`, and the time between is unknown (SPEC §7.2).
const MAX_GAP: SignedDuration = SignedDuration::from_hours(3 * 24);

/// The price history of every listing: its series (see [`stats::merge`]) and the
/// reference values computed from it.
///
/// History is kept per listing, not per (product, chain): a listing's history starts when
/// it is first seen, so comparing a 4-pack seen today with a single can seen last week
/// would look like a price drop that never happened.
#[derive(Debug, Clone, Default)]
pub struct History {
    pub series: HashMap<i64, Vec<Interval>>,
    pub references: HashMap<i64, References>,
    /// The product and chain of each listing.
    pub listings: HashMap<i64, (ProductId, Chain)>,
}

impl History {
    /// Builds the history from the stored intervals. `liter_price` gives the ranked liter
    /// price of a stored price on a date, or `None` to leave it out.
    pub fn build(
        history: &[HistoricPrice],
        now: Timestamp,
        liter_price: impl Fn(&StoredPrice, Date) -> Option<Ore>,
    ) -> Self {
        let mut built = Self::default();
        for listing in history.chunk_by(|a, b| a.price.listing_id == b.price.listing_id) {
            let first = &listing[0].price;
            let series = stats::merge(&intervals(listing, &liter_price));
            if series.is_empty() {
                continue;
            }
            built
                .references
                .insert(first.listing_id, stats::compute(&series, now));
            built.series.insert(first.listing_id, series);
            built
                .listings
                .insert(first.listing_id, (first.product.clone(), first.chain));
        }
        built
    }

    pub fn references(&self, listing_id: i64) -> References {
        self.references
            .get(&listing_id)
            .copied()
            .unwrap_or_default()
    }

    pub fn series(&self, listing_id: i64) -> &[Interval] {
        self.series.get(&listing_id).map_or(&[], Vec::as_slice)
    }

    /// The listings of `product` at `chain` that have history.
    pub fn listings_of(&self, product: &ProductId, chain: Chain) -> Vec<i64> {
        let mut ids: Vec<i64> = self
            .listings
            .iter()
            .filter(|(_, (p, c))| p == product && *c == chain)
            .map(|(&id, _)| id)
            .collect();
        ids.sort_unstable();
        ids
    }

    pub fn has_history(&self, product: &ProductId) -> bool {
        self.listings.values().any(|(p, _)| p == product)
    }

    /// The Trend column: the price in `slots` slots over the 90-day window, or nothing
    /// when there is too little history to show a trend (SPEC §7.9).
    pub fn trend(
        &self,
        listing_id: i64,
        now: Timestamp,
        slots: usize,
        min_coverage_days: u32,
    ) -> Vec<Option<Ore>> {
        if self.references(listing_id).coverage_days < min_coverage_days {
            return Vec::new();
        }
        trend::slots(
            self.series(listing_id),
            stats::window_start(now),
            now,
            slots,
        )
    }
}

/// The intervals of one listing, oldest first.
fn intervals(
    listing: &[HistoricPrice],
    liter_price: impl Fn(&StoredPrice, Date) -> Option<Ore>,
) -> Vec<Interval> {
    let mut intervals = Vec::new();
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
        intervals.push(Interval {
            liter_price,
            from: entry.valid_from,
            to: to.max(entry.valid_from),
        });
    }
    intervals
}

fn oslo() -> TimeZone {
    TimeZone::get("Europe/Oslo").unwrap_or(TimeZone::UTC)
}

/// `time` in Norway.
pub fn oslo_time(time: Timestamp) -> Zoned {
    time.to_zoned(oslo())
}

/// The date in Norway at `time`. Day boundaries always follow Europe/Oslo (SPEC §7.2).
pub fn oslo_date(time: Timestamp) -> Date {
    oslo_time(time).date()
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
        History::build(history, day(0), |p, _| Some(p.shelf_price)).references(1)
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
    fn each_listing_has_its_own_history() {
        let history = [
            entry(1, 65, -30, 0),
            entry(2, 45, -1, 0), // a 4-pack, first seen yesterday
        ];
        let built = History::build(&history, day(0), |p, _| Some(p.shelf_price));
        // The 4-pack has no earlier price, so it is not a drop from 65.
        assert_eq!(built.references(2).m90, None);
        assert_eq!(built.references(2).before_current, None);
        assert_eq!(
            built.listings_of(&ProductId("monster-ultra-white-05".into()), Chain::Meny),
            [1, 2]
        );
    }
}
