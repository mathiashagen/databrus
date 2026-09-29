//! One module per command. Each has a `run` function that returns an exit status.

pub mod config;
pub mod deals;
pub mod export;
pub mod history;
pub mod products;
pub mod schedule;
pub mod search;
pub mod stores;
pub mod update;
pub mod watch;

use crate::db::Database;
use crate::error::AppError;

/// Commands that show prices exit with 3 when there are none (SPEC §8).
pub(crate) fn require_price_data(db: &Database) -> Result<(), AppError> {
    if db.has_price_data()? {
        Ok(())
    } else {
        Err(AppError::NoData)
    }
}
