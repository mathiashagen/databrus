//! Price history: reference prices, verdicts and deal detection (SPEC §7.6–7.9).

pub mod deals;
pub mod stats;
pub mod verdict;

pub use stats::{Interval, References};
pub use verdict::{Thresholds, assess};
