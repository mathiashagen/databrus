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

use owo_colors::OwoColorize;

use crate::Context;
use crate::alerts;
use crate::catalog::Catalog;
use crate::catalog::parse::normalize;
use crate::cli::SearchArgs;
use crate::db::Database;
use crate::error::AppError;
use crate::history::{History, deals as deal_dates};
use crate::model::{self, Chain, Product, ProductId, SearchResult};
use crate::output::format;
use crate::search::SearchFilter;
use crate::search::ranking;
use crate::sources::SourceStatus;

/// Commands that show prices exit with 3 when there are none (SPEC §8).
pub(crate) fn require_price_data(db: &Database) -> Result<(), AppError> {
    if db.has_price_data()? {
        Ok(())
    } else {
        Err(AppError::NoData)
    }
}

/// Finds the one product the user means (`historikk`, `overvak legg-til`): by id, or by
/// text as in search, preferring an exact name and then products with prices. More than
/// one match is an error that lists them.
pub(crate) fn resolve_product<'a>(
    catalog: &'a Catalog,
    query: &[String],
    has_prices: impl Fn(&ProductId) -> bool,
    ctx: &Context,
) -> Result<&'a Product, AppError> {
    let text = query.join(" ");
    if let Some(product) = catalog.products.iter().find(|p| p.id.0 == text.trim()) {
        return Ok(product);
    }
    let filter = SearchFilter::from_args(
        &SearchArgs {
            query: query.to_vec(),
            ..SearchArgs::default()
        },
        &ctx.config,
    );
    let mut candidates: Vec<&Product> = catalog
        .products
        .iter()
        .filter(|p| filter.product_matches(p))
        .collect();
    narrow(&mut candidates, |p| normalize(&p.name) == normalize(&text));
    narrow(&mut candidates, |p| has_prices(&p.id));

    match candidates.as_slice() {
        [] => Err(AppError::Usage(format!(
            "ingen produkter passer «{text}» – se `databrus produkter`"
        ))),
        [one] => Ok(one),
        many => {
            let mut many = many.to_vec();
            many.sort_by(|a, b| a.name.cmp(&b.name).then(a.volume.cmp(&b.volume)));
            let mut list: Vec<String> = many
                .iter()
                .take(10)
                .map(|p| format!("  {} {}  ({})", p.name, format::liters(p.volume), p.id.0))
                .collect();
            if many.len() > 10 {
                list.push(format!("  … og {} til", many.len() - 10));
            }
            Err(AppError::Usage(format!(
                "flere produkter passer «{text}» – skriv mer presist eller bruk id-en:\n{}",
                list.join("\n")
            )))
        }
    }
}

/// Keeps only the candidates that pass `keep`, unless that would leave none.
fn narrow(candidates: &mut Vec<&Product>, keep: impl Fn(&Product) -> bool) {
    if candidates.len() > 1 && candidates.iter().any(|p| keep(p)) {
        candidates.retain(|p| keep(p));
    }
}

/// The current price rows of every product at every chain, as search would rank them
/// (fresh, available, not suspicious), without a row limit. For alerts, so no history.
pub(crate) fn current_rows(
    db: &Database,
    catalog: &Catalog,
    ctx: &Context,
) -> Result<Vec<SearchResult>, AppError> {
    let mut filter = SearchFilter::from_args(&SearchArgs::default(), &ctx.config);
    filter.chains = Chain::ALL.to_vec();
    filter.limit = usize::MAX;
    let hits = ranking::rank(
        &db.latest_prices()?,
        &History::default(),
        catalog,
        &filter,
        &ctx.config,
        model::now(),
        deal_dates::today_oslo(),
    );
    Ok(hits.rows)
}

/// Checks the price alerts after a fetch that got new data (SPEC §7.7): each one that
/// fires is shown on stderr and, unless turned off, as a desktop notification.
pub(crate) fn check_alerts(
    db: &mut Database,
    catalog: &Catalog,
    ctx: &Context,
    statuses: &[SourceStatus],
) -> Result<(), AppError> {
    if !statuses.iter().any(|s| s.new_listings.is_some()) {
        return Ok(());
    }
    let alerts = db.alerts()?;
    if alerts.is_empty() {
        return Ok(());
    }
    let rows = current_rows(db, catalog, ctx)?;
    for (alert, row) in alerts::to_fire(&alerts, &rows) {
        let (title, body) = alerts::message(alert, row);
        anstream::eprintln!("{} {title}: {body}", "varsel:".yellow().bold());
        if ctx.config.alerts.desktop {
            notify_desktop(&title, &body);
        }
        db.mark_alert_triggered(alert.id, row.interval_id)?;
    }
    Ok(())
}

/// A desktop notification. Failing to show one is not an error: the line on stderr is
/// already there.
fn notify_desktop(title: &str, body: &str) {
    let shown = notify_rust::Notification::new()
        .appname("databrus")
        .summary(title)
        .body(body)
        .show();
    if let Err(e) = shown {
        tracing::warn!("kunne ikke vise skrivebordsvarsel: {e}");
    }
}
