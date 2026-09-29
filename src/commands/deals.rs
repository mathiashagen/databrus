//! `databrus tilbud`

use crate::Context;
use crate::cli::DealsArgs;
use crate::error::{AppError, ExitStatus};
use crate::search::SearchFilter;
use crate::sources;

pub async fn run(args: &DealsArgs, ctx: &Context) -> Result<ExitStatus, AppError> {
    let _filter = SearchFilter::from_args(&args.filters, &ctx.config);
    let catalog = ctx.catalog()?;
    let mut db = ctx.open_database()?;
    let _sources = sources::refresh(&mut db, &ctx.config, &catalog, ctx.fetch_mode(), &[]).await?;
    super::require_price_data(&db)?;

    // M2: KAMPANJE/PRISFALL, sorted by verdict and then liter price.
    Err(AppError::NotImplemented("tilbudsvisning, M2"))
}
