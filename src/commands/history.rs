//! `databrus historikk <produkt>` – the price over time per chain (SPEC §7.9). Reads local
//! data only.

use std::io::Write;

use jiff::{Timestamp, ToSpan};
use serde::Serialize;

use crate::Context;
use crate::cli::{HistoryArgs, SearchArgs};
use crate::error::{AppError, ExitStatus};
use crate::history::{History, deals, oslo_date, start_of_day};
use crate::model::{self, Chain, Ore, Product, Verdict, VerdictInfo};
use crate::output::chart::{self, Series};
use crate::output::json::{self, Envelope, Header};
use crate::output::{self, OutputFormat, format, table};
use crate::search::SearchFilter;
use crate::search::ranking;

/// Rows in the chart. Odd, so the middle label is exactly halfway.
const CHART_HEIGHT: usize = 11;
const MAX_CHART_WIDTH: usize = 100;
/// Room for the price labels and the axis to the left of the chart.
const AXIS_WIDTH: usize = 10;

#[derive(Serialize)]
struct HistoryContent<'a> {
    #[serde(rename = "produkt")]
    product: &'a Product,
    #[serde(rename = "dager")]
    days: u32,
    #[serde(rename = "serier")]
    series: Vec<ChainSeries>,
}

/// One chain: its price intervals in the period and the reference values.
#[derive(Serialize)]
struct ChainSeries {
    #[serde(rename = "kjede")]
    chain: Chain,
    #[serde(skip)]
    listing_id: i64,
    /// The current liter price, if the chain still has one.
    #[serde(rename = "literpris_ore")]
    liter_price: Option<Ore>,
    #[serde(rename = "vurdering")]
    verdict: VerdictInfo,
    #[serde(rename = "intervaller")]
    intervals: Vec<IntervalOut>,
}

#[derive(Serialize)]
struct IntervalOut {
    #[serde(rename = "fra")]
    from: Timestamp,
    #[serde(rename = "til")]
    to: Timestamp,
    #[serde(rename = "literpris_ore")]
    liter_price: Ore,
}

pub fn run(args: &HistoryArgs, ctx: &Context) -> Result<ExitStatus, AppError> {
    let catalog = ctx.catalog()?;
    let db = ctx.open_database()?;
    super::require_price_data(&db)?;

    let now = model::now();
    let history = ranking::history(&db.price_history()?, &catalog, &ctx.config, now);
    let product =
        super::resolve_product(&catalog, &args.product, |id| history.has_history(id), ctx)?;

    // Current prices and verdicts for every chain, computed the same way as in search:
    // stale, sold-out and suspicious prices are not current, but no row limit.
    let mut filter = SearchFilter::from_args(
        &SearchArgs {
            stores: args.chains.clone(),
            ..SearchArgs::default()
        },
        &ctx.config,
    );
    filter.limit = usize::MAX;
    let hits = ranking::rank(
        &db.latest_prices()?,
        &history,
        &catalog,
        &filter,
        &ctx.config,
        now,
        deals::today_oslo(),
    );

    let days = args.days.max(1);
    let start = start_of_day(oslo_date(now) - (i64::from(days) - 1).days());
    let series: Vec<ChainSeries> = filter
        .chains
        .iter()
        .filter_map(|&chain| {
            let current = hits
                .rows
                .iter()
                .find(|r| r.product.id == product.id && r.chain == chain);
            // The listing shown today, or else the one seen most recently.
            let listing = current.map(|r| r.listing_id).or_else(|| {
                history
                    .listings_of(&product.id, chain)
                    .into_iter()
                    .max_by_key(|&id| history.series(id).last().map(|i| i.to))
            })?;
            let intervals: Vec<IntervalOut> = history
                .series(listing)
                .iter()
                .filter(|i| i.to >= start && i.from <= now)
                .map(|i| IntervalOut {
                    from: i.from,
                    to: i.to,
                    liter_price: i.liter_price,
                })
                .collect();
            if intervals.is_empty() && current.is_none() {
                return None;
            }
            let verdict = current.map_or_else(
                || {
                    let refs = history.references(listing);
                    VerdictInfo {
                        value: Verdict::Unknown,
                        l30_ore: refs.l30,
                        m90_ore: refs.m90,
                        atl_ore: refs.atl,
                        coverage_days: refs.coverage_days,
                    }
                },
                |r| r.verdict.clone(),
            );
            Some(ChainSeries {
                chain,
                listing_id: listing,
                liter_price: current.map(|r| r.liter_price),
                verdict,
                intervals,
            })
        })
        .collect();

    match ctx.output_format() {
        OutputFormat::Json => json::write(&Envelope::new(HistoryContent {
            product,
            days,
            series,
        }))?,
        OutputFormat::JsonLines => json::write_lines(&Header::new(), &series)?,
        OutputFormat::Table => write_table(product, &history, &series, start, now, days)?,
    }
    Ok(ExitStatus::Ok)
}

fn write_table(
    product: &Product,
    history: &History,
    series: &[ChainSeries],
    start: Timestamp,
    now: Timestamp,
    days: u32,
) -> Result<(), AppError> {
    let name = format!("{} {}", product.name, format::liters(product.volume));
    if series.is_empty() {
        eprintln!("ingen priser for {name} ennå");
        return Ok(());
    }
    let mut out = anstream::stdout();
    writeln!(out, "{name} – literpris siste {days} dager\n")?;

    let color = output::colors_enabled();
    let width = table::terminal_width().map_or(80, usize::from);
    let chart_width = width.saturating_sub(AXIS_WIDTH).clamp(20, MAX_CHART_WIDTH);
    let lines: Vec<Series<'_>> = series
        .iter()
        .map(|s| Series {
            label: s.chain.display_name().to_owned(),
            intervals: history.series(s.listing_id),
        })
        .collect();
    match chart::render(&lines, start, now, chart_width, CHART_HEIGHT, color) {
        Some(chart) => writeln!(out, "{chart}\n")?,
        None => writeln!(out, "ingen priser i perioden\n")?,
    }

    let kr = |value: Option<Ore>| value.map_or("–".to_owned(), format::kr);
    let mut t = table::new(&["Kjede", "Nå", "L30", "M90", "ATL", "Vurdering", "Dekning"]);
    for column in 1..=4 {
        if let Some(c) = t.column_mut(column) {
            c.set_cell_alignment(comfy_table::CellAlignment::Right);
        }
    }
    for s in series {
        t.add_row(vec![
            s.chain.display_name().to_owned(),
            kr(s.liter_price),
            kr(s.verdict.l30_ore),
            kr(s.verdict.m90_ore),
            kr(s.verdict.atl_ore),
            s.verdict.value.label().to_owned(),
            format!("{} d", s.verdict.coverage_days),
        ]);
    }
    writeln!(out, "{t}")?;
    Ok(())
}
