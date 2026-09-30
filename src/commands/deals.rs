//! `databrus tilbud` – current deals, best verdict first (SPEC §7.8).

use crate::Context;
use crate::cli::DealsArgs;
use crate::error::{AppError, ExitStatus};
use crate::history::deals;
use crate::model;
use crate::output::OutputFormat;
use crate::output::json::{self, Envelope, Header};
use crate::search::ranking::{self, SearchHits};
use crate::search::{SearchContent, SearchFilter};
use crate::sources;

pub async fn run(args: &DealsArgs, ctx: &Context) -> Result<ExitStatus, AppError> {
    let mut filter = SearchFilter::from_args(&args.filters, &ctx.config);
    filter.deals_only = true;
    filter.upcoming = args.upcoming;
    let catalog = ctx.catalog()?;
    let mut db = ctx.open_database()?;
    let statuses = sources::refresh(&mut db, &ctx.config, &catalog, ctx.fetch_mode(), &[]).await?;
    super::require_price_data(&db)?;

    let now = model::now();
    let references = ranking::references(&db.price_history()?, &catalog, &ctx.config, now);
    let hits = ranking::rank(
        &db.latest_prices()?,
        &references,
        &catalog,
        &filter,
        &ctx.config,
        now,
        deals::today_oslo(),
    );

    // The same document as the search, so the published schema covers it (SPEC §9.3).
    match ctx.output_format() {
        OutputFormat::Json => json::write(&Envelope {
            header: Header::new().with_sources(statuses.clone()),
            content: SearchContent {
                query: filter.query(&args.filters.query),
                results: &hits.rows,
            },
        })?,
        OutputFormat::JsonLines => {
            json::write_lines(&Header::new().with_sources(statuses.clone()), &hits.rows)?;
        }
        OutputFormat::Table if !hits.rows.is_empty() => {
            super::search::write_table(&hits, &statuses, now)?;
        }
        OutputFormat::Table => {}
    }
    if hits.rows.is_empty() {
        no_deals(&hits);
    }

    Ok(super::search::exit_status(
        ctx.global.strict,
        &hits,
        &statuses,
    ))
}

fn no_deals(hits: &SearchHits) {
    let hidden = hits.hidden.total();
    if hidden > 0 {
        eprintln!(
            "ingen tilbud – {hidden} er skjult fordi de er gamle, utsolgte eller mistenkelige (--alle viser dem)"
        );
    } else {
        eprintln!("ingen tilbud akkurat nå");
    }
}
