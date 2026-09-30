//! Reference prices from the history: L30, M90 and ATL (SPEC §7.6).

use std::collections::BTreeSet;

use jiff::civil::Date;
use jiff::{Timestamp, ToSpan};

use super::{oslo_date, start_of_day};
use crate::model::Ore;

/// The window for M90 and the coverage count, in days including today.
pub const WINDOW_DAYS: i32 = 90;
/// The window for L30, in days before the current price began.
pub const L30_DAYS: i32 = 30;

/// The reference values for one (product, chain).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct References {
    /// The lowest price in the 30 days before the current price began (the Omnibus rule).
    pub l30: Option<Ore>,
    /// The time-weighted median over the last 90 days, excluding the current interval.
    pub m90: Option<Ore>,
    /// The lowest price in the whole local history, excluding the current interval.
    pub atl: Option<Ore>,
    /// The price just before the current interval, e.g. the price before a deal.
    pub before_current: Option<Ore>,
    /// Days with a known price in the last 90 days.
    pub coverage_days: u32,
}

/// One interval in which the liter price was known. Gaps in the history are unknown, not
/// "same price as last time" (SPEC §7.2). `from == to` is a single observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interval {
    pub liter_price: Ore,
    pub from: Timestamp,
    pub to: Timestamp,
}

/// Merges the intervals of several listings (say a single can and a 4-pack) into one
/// series: at every moment the lowest known price, which is what the ranking shows.
/// The result is sorted, has no overlaps, and neighbours with the same price are joined.
pub fn merge(intervals: &[Interval]) -> Vec<Interval> {
    let mut bounds: Vec<Timestamp> = intervals.iter().flat_map(|i| [i.from, i.to]).collect();
    bounds.sort();
    bounds.dedup();

    let mut series: Vec<Interval> = Vec::new();
    for pair in bounds.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        let lowest = intervals
            .iter()
            .filter(|i| i.from <= from && to <= i.to)
            .map(|i| i.liter_price)
            .min();
        if let Some(liter_price) = lowest {
            series.push(Interval {
                liter_price,
                from,
                to,
            });
        }
    }
    // Single observations count where nothing longer covers them.
    for point in intervals.iter().filter(|i| i.from == i.to) {
        if !series
            .iter()
            .any(|s| s.from <= point.from && point.from <= s.to)
        {
            series.push(*point);
        }
    }
    series.sort_by_key(|s| (s.from, s.to));

    let mut joined: Vec<Interval> = Vec::with_capacity(series.len());
    for segment in series {
        match joined.last_mut() {
            Some(last) if last.to == segment.from && last.liter_price == segment.liter_price => {
                last.to = segment.to;
            }
            _ => joined.push(segment),
        }
    }
    joined
}

/// Computes the reference values from a merged series (see [`merge`]). The last interval
/// is the current price.
pub fn compute(series: &[Interval], now: Timestamp) -> References {
    let Some((current, earlier)) = series.split_last() else {
        return References::default();
    };
    let window_start = start_of_day(oslo_date(now) - (WINDOW_DAYS - 1).days());
    let l30_start = start_of_day(oslo_date(current.from) - L30_DAYS.days());

    let l30 = earlier
        .iter()
        .filter(|i| i.to >= l30_start && i.from < current.from)
        .map(|i| i.liter_price)
        .min();
    let atl = earlier.iter().map(|i| i.liter_price).min();

    let in_window: Vec<(Ore, i64)> = earlier
        .iter()
        .filter(|i| i.to >= window_start && i.from <= now)
        .map(|i| {
            let from = i.from.max(window_start);
            let to = i.to.min(now);
            (i.liter_price, to.duration_since(from).as_secs().max(0))
        })
        .collect();

    References {
        l30,
        m90: weighted_median(&in_window),
        atl,
        before_current: earlier.last().map(|i| i.liter_price),
        coverage_days: coverage_days(series, window_start, now),
    }
}

/// The price at which half the known time has a lower or equal price. When no interval
/// has any length (only single observations), every price counts once.
fn weighted_median(prices: &[(Ore, i64)]) -> Option<Ore> {
    let mut prices = prices.to_vec();
    if prices.iter().all(|&(_, weight)| weight == 0) {
        prices.iter_mut().for_each(|(_, weight)| *weight = 1);
    }
    prices.sort();
    let total: i64 = prices.iter().map(|&(_, weight)| weight).sum();
    let mut seen = 0;
    for (price, weight) in prices {
        seen += weight;
        if seen * 2 >= total {
            return Some(price);
        }
    }
    None
}

/// Days in Europe/Oslo between `start` and `now` on which some price was known.
fn coverage_days(series: &[Interval], start: Timestamp, now: Timestamp) -> u32 {
    let mut days: BTreeSet<Date> = BTreeSet::new();
    for interval in series.iter().filter(|i| i.to >= start && i.from <= now) {
        let mut day = oslo_date(interval.from.max(start));
        let last = oslo_date(interval.to.min(now));
        while day <= last {
            days.insert(day);
            match day.tomorrow() {
                Ok(next) => day = next,
                Err(_) => break,
            }
        }
    }
    u32::try_from(days.len()).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use jiff::SignedDuration;
    use jiff::civil::date;

    use super::*;

    /// Noon in Oslo on `day` of October 2026 (negative days reach back into September
    /// and earlier).
    fn at(day: i32) -> Timestamp {
        start_of_day(date(2026, 10, 1) + (day - 1).days()) + SignedDuration::from_hours(12)
    }

    fn interval(kr: i64, from: i32, to: i32) -> Interval {
        Interval {
            liter_price: Ore(kr * 100),
            from: at(from),
            to: at(to),
        }
    }

    #[test]
    fn empty_history_has_no_references() {
        assert_eq!(compute(&[], at(1)), References::default());
    }

    #[test]
    fn a_single_interval_has_coverage_but_no_references() {
        let refs = compute(&[interval(40, -20, 1)], at(1));
        assert_eq!(refs.coverage_days, 22);
        assert_eq!((refs.l30, refs.m90, refs.atl), (None, None, None));
        assert_eq!(refs.before_current, None);
    }

    #[test]
    fn references_exclude_the_current_interval() {
        let series = [
            interval(50, -60, -40),
            interval(40, -40, -10),
            interval(44, -10, -3),
            interval(30, -3, 1),
        ];
        let refs = compute(&series, at(1));
        // 30 days before day -3: day -33 onwards, so 50 (ended day -40) is outside.
        assert_eq!(refs.l30, Some(Ore(4000)));
        assert_eq!(refs.atl, Some(Ore(4000)));
        assert_eq!(refs.before_current, Some(Ore(4400)));
        // 20 days at 50, 30 at 40, 7 at 44: the middle of 57 days is at 40.
        assert_eq!(refs.m90, Some(Ore(4000)));
        assert_eq!(refs.coverage_days, 62);
    }

    #[test]
    fn median_is_weighted_by_time() {
        // 5 days at 30, 20 days at 45: the median is 45 even though 30 is lower.
        let series = [
            interval(30, -30, -25),
            interval(45, -25, -5),
            interval(38, -5, 1),
        ];
        assert_eq!(compute(&series, at(1)).m90, Some(Ore(4500)));
    }

    #[test]
    fn median_only_looks_at_the_last_90_days() {
        let series = [
            interval(20, -200, -100),
            interval(45, -60, -5),
            interval(38, -5, 1),
        ];
        let refs = compute(&series, at(1));
        assert_eq!(refs.m90, Some(Ore(4500)));
        assert_eq!(refs.atl, Some(Ore(2000)));
    }

    #[test]
    fn gaps_do_not_count_as_coverage() {
        let series = [interval(40, -30, -25), interval(40, -5, 1)];
        // Days -30..=-25 and -5..=1.
        assert_eq!(compute(&series, at(1)).coverage_days, 6 + 7);
    }

    #[test]
    fn single_observations_count_once() {
        let series = [
            interval(50, -10, -10),
            interval(40, -9, -9),
            interval(42, -8, -8),
            interval(30, 1, 1),
        ];
        let refs = compute(&series, at(1));
        assert_eq!(refs.m90, Some(Ore(4200)));
        assert_eq!(refs.coverage_days, 4);
    }

    #[test]
    fn merge_takes_the_lowest_price_at_each_moment() {
        // A single can at 40 throughout, and a 4-pack at 35 for part of the time.
        let merged = merge(&[interval(40, -20, 1), interval(35, -10, -5)]);
        assert_eq!(
            merged,
            [
                interval(40, -20, -10),
                interval(35, -10, -5),
                interval(40, -5, 1)
            ]
        );
    }

    #[test]
    fn merge_joins_equal_neighbours_and_keeps_gaps() {
        let merged = merge(&[
            interval(40, -20, -10),
            interval(40, -10, -5),
            interval(40, -3, 1),
        ]);
        assert_eq!(merged, [interval(40, -20, -5), interval(40, -3, 1)]);
    }

    #[test]
    fn merge_keeps_single_observations_outside_other_intervals() {
        let merged = merge(&[
            interval(40, -20, -10),
            interval(38, -15, -15),
            interval(30, 1, 1),
        ]);
        assert_eq!(merged, [interval(40, -20, -10), interval(30, 1, 1)]);
    }
}
