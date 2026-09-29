//! Search (the default command).

use std::io::Write;

use owo_colors::OwoColorize;
use serde::Serialize;

use crate::Context;
use crate::cli::SearchArgs;
use crate::error::{AppError, ExitStatus};
use crate::history::deals;
use crate::model::{self, SearchResult};
use crate::output::json::{self, Envelope, Header};
use crate::output::{self, OutputFormat, results, table};
use crate::search::ranking::{self, SearchHits};
use crate::search::{Query, SearchFilter, SortBy};
use crate::sources::{self, SourceState, SourceStatus};

/// A row older than this counts as stale for `--streng` (SPEC §8).
const STALE_HOURS: u32 = 24;

#[derive(Serialize)]
struct SearchContent<'a> {
    #[serde(rename = "sporring")]
    query: Query,
    #[serde(rename = "resultater")]
    results: &'a [SearchResult],
}

pub async fn run(args: &SearchArgs, ctx: &Context) -> Result<ExitStatus, AppError> {
    let filter = SearchFilter::from_args(args, &ctx.config);
    if filter.sort == SortBy::Discount {
        anstream::eprintln!(
            "{} --sorter rabatt krever prishistorikk (kommer i M2) – sorterer etter literpris",
            "info:".cyan().bold()
        );
    }
    let catalog = ctx.catalog()?;
    let mut db = ctx.open_database()?;
    let statuses = sources::refresh(&mut db, &ctx.config, &catalog, ctx.fetch_mode(), &[]).await?;
    super::require_price_data(&db)?;

    let now = model::now();
    let hits = ranking::rank(
        &db.latest_prices()?,
        &catalog,
        &filter,
        &ctx.config,
        now,
        deals::today_oslo(),
    );

    match ctx.output_format() {
        OutputFormat::Json => json::write(&Envelope {
            header: Header::new().with_sources(statuses.clone()),
            content: SearchContent {
                query: filter.query(&args.query),
                results: &hits.rows,
            },
        })?,
        OutputFormat::JsonLines => {
            json::write_lines(&Header::new().with_sources(statuses.clone()), &hits.rows)?;
        }
        OutputFormat::Table if !hits.rows.is_empty() => write_table(&hits, &statuses, now)?,
        OutputFormat::Table => {}
    }
    if hits.rows.is_empty() {
        no_hits(&hits);
    }

    Ok(exit_status(ctx.global.strict, &hits, &statuses))
}

fn write_table(
    hits: &SearchHits,
    statuses: &[SourceStatus],
    now: jiff::Timestamp,
) -> Result<(), AppError> {
    let color = output::colors_enabled();
    let t = results::table(hits, color, table::terminal_width());
    let mut out = anstream::stdout();
    writeln!(out, "{t}")?;
    let footer = results::footer(hits, statuses, now);
    if color {
        writeln!(out, "{}", footer.dimmed())?;
    } else {
        writeln!(out, "{footer}")?;
    }
    Ok(())
}

fn no_hits(hits: &SearchHits) {
    let hidden = hits.hidden.total();
    if hidden > 0 {
        eprintln!(
            "ingen treff – {hidden} er skjult fordi de er gamle, utsolgte eller mistenkelige (--alle viser dem)"
        );
    } else {
        eprintln!("ingen treff");
    }
}

/// `--streng` exits with 2 when a source failed or a shown row is stale.
fn exit_status(strict: bool, hits: &SearchHits, statuses: &[SourceStatus]) -> ExitStatus {
    let failed = statuses.iter().any(|s| s.status == SourceState::Failed);
    let stale = hits.rows.iter().any(|r| r.age_hours > STALE_HOURS);
    if strict && (failed || stale) {
        ExitStatus::Strict
    } else {
        ExitStatus::Ok
    }
}
