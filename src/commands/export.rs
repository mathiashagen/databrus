//! `databrus eksporter` – reads local data only.

use crate::Context;
use crate::cli::ExportArgs;
use crate::error::{AppError, ExitStatus};

pub fn run(_args: &ExportArgs, ctx: &Context) -> Result<ExitStatus, AppError> {
    let db = ctx.open_database()?;
    super::require_price_data(&db)?;

    // M2: CSV with one price interval per row.
    Err(AppError::NotImplemented("CSV-eksport, M2"))
}
