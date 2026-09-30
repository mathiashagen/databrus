//! The price over time in a few slots, for the Trend column and `historikk` (SPEC §7.9).

use jiff::Timestamp;

use super::Interval;
use crate::model::Ore;
use crate::pricing::div_round;

/// Splits `start..end` into `count` equal slots and gives the time-weighted average price
/// in each, or `None` where no price was known. A single observation counts as one second.
pub fn slots(
    series: &[Interval],
    start: Timestamp,
    end: Timestamp,
    count: usize,
) -> Vec<Option<Ore>> {
    let span = end.duration_since(start).as_secs();
    if count == 0 || span <= 0 {
        return vec![None; count];
    }
    let count_i = i64::try_from(count).unwrap_or(i64::MAX);
    (0..count_i)
        .map(|i| {
            let slot_start = start.as_second() + span * i / count_i;
            let slot_end = start.as_second() + span * (i + 1) / count_i;
            let mut weighted: i128 = 0;
            let mut total: i128 = 0;
            for interval in series {
                let from = interval.from.as_second().max(slot_start);
                let to = interval.to.as_second().min(slot_end);
                let point = interval.from == interval.to;
                let weight = match to - from {
                    0 if point => 1,
                    length if length > 0 => i128::from(length),
                    _ => continue,
                };
                weighted += i128::from(interval.liter_price.0) * weight;
                total += weight;
            }
            (total > 0).then(|| Ore(div_round(weighted, total)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use jiff::SignedDuration;

    use super::*;

    fn day(n: i64) -> Timestamp {
        Timestamp::from_second(1_790_000_000).unwrap() + SignedDuration::from_hours(24 * n)
    }

    fn interval(kr: i64, from: i64, to: i64) -> Interval {
        Interval {
            liter_price: Ore(kr * 100),
            from: day(from),
            to: day(to),
        }
    }

    #[test]
    fn each_slot_averages_its_own_time() {
        let series = [interval(40, 0, 3), interval(30, 3, 6)];
        let slots = slots(&series, day(0), day(6), 3);
        // Days 0–2 at 40, 2–4 half and half, 4–6 at 30.
        assert_eq!(slots, [Some(Ore(4000)), Some(Ore(3500)), Some(Ore(3000))]);
    }

    #[test]
    fn slots_without_prices_are_empty() {
        let series = [interval(40, 0, 1), interval(30, 5, 6)];
        let slots = slots(&series, day(0), day(6), 3);
        assert_eq!(slots, [Some(Ore(4000)), None, Some(Ore(3000))]);
    }

    #[test]
    fn a_single_observation_fills_its_slot() {
        let series = [interval(40, 4, 4)];
        assert_eq!(slots(&series, day(0), day(6), 2), [None, Some(Ore(4000))]);
    }
}
