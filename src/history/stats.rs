//! Reference prices from the history: L30, M90 and ATL (SPEC §7.6). Computed in M2.

use jiff::Timestamp;

use crate::model::Ore;

/// The reference values for one (product, chain).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct References {
    /// The lowest price in the 30 days before the current price began (the Omnibus rule).
    pub l30: Option<Ore>,
    /// The time-weighted median over the last 90 days, excluding the current interval.
    pub m90: Option<Ore>,
    /// The lowest price in the whole local history.
    pub atl: Option<Ore>,
    /// Days with a known price in the last 90 days.
    pub coverage_days: u32,
}

/// One interval in which the liter price was known. Gaps in the history are unknown, not
/// "same price as last time" (SPEC §7.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interval {
    pub liter_price: Ore,
    pub from: Timestamp,
    pub to: Timestamp,
}

/// Computes the reference values. Until M2 is implemented, there is no coverage.
pub fn compute(_intervals: &[Interval], _now: Timestamp) -> References {
    References::default()
}
