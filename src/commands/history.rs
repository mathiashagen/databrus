//! `databrus historikk <produkt>` – reads local data only.

use crate::Context;
use crate::cli::HistoryArgs;
use crate::error::{AppError, ExitStatus};

pub fn run(_args: &HistoryArgs, ctx: &Context) -> Result<ExitStatus, AppError> {
    let db = ctx.open_database()?;
    super::require_price_data(&db)?;

    // M2: a chart per chain and a table with now/L30/M90/ATL.
    Err(AppError::NotImplemented("historikkgraf, M2"))
}
