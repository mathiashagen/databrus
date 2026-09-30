//! `databrus overvak …` – price alerts (SPEC §7.7). Alerts are checked after every fetch
//! that gets new data, see [`super::check_alerts`].

use std::collections::HashSet;

use serde::Serialize;

use crate::Context;
use crate::alerts::{Alert, NewAlert, ThresholdKind};
use crate::catalog::Catalog;
use crate::cli::{AddAlertArgs, WatchCommand};
use crate::error::{AppError, ExitStatus};
use crate::model::{self, Chain, Ore, SearchResult};
use crate::output::json::{self, Envelope, Header};
use crate::output::{OutputFormat, format, table};

/// An alert with the price it would compare today.
#[derive(Serialize)]
struct AlertView<'a> {
    #[serde(flatten)]
    alert: &'a Alert,
    #[serde(rename = "produktnavn")]
    name: String,
    #[serde(rename = "beste_pris_ore")]
    best_price: Option<Ore>,
    #[serde(rename = "beste_kjede")]
    best_chain: Option<Chain>,
    #[serde(rename = "under_grensen")]
    below: bool,
}

#[derive(Serialize)]
struct AlertContent<'a> {
    #[serde(rename = "varsel")]
    alert: AlertView<'a>,
}

#[derive(Serialize)]
struct AlertsContent<'a> {
    #[serde(rename = "varsler")]
    alerts: Vec<AlertView<'a>>,
}

pub fn run(command: &WatchCommand, ctx: &Context) -> Result<ExitStatus, AppError> {
    match command {
        WatchCommand::Add(args) => add(args, ctx),
        WatchCommand::List => list(ctx),
        WatchCommand::Remove { id } => remove(*id, ctx),
    }
}

fn add(args: &AddAlertArgs, ctx: &Context) -> Result<ExitStatus, AppError> {
    let catalog = ctx.catalog()?;
    let mut db = ctx.open_database()?;
    // The alert refers to the product, which may not be stored yet before a first fetch.
    db.sync_catalog(&catalog)?;
    let rows = super::current_rows(&db, &catalog, ctx)?;
    let priced: HashSet<_> = rows.iter().map(|r| &r.product.id).collect();
    let product = super::resolve_product(&catalog, &args.product, |id| priced.contains(id), ctx)?;

    let new = NewAlert {
        product: product.id.clone(),
        chain: args.chain,
        threshold: args.below,
        kind: if args.liter_price {
            ThresholdKind::LiterPrice
        } else {
            ThresholdKind::UnitPrice
        },
    };
    let created = model::now();
    let id = db.add_alert(&new, created)?;
    let alert = Alert {
        id,
        product: new.product,
        chain: new.chain,
        threshold: new.threshold,
        kind: new.kind,
        created,
        last_triggered_interval: None,
    };
    let view = view(&alert, &catalog, &rows);

    match ctx.output_format() {
        OutputFormat::Json | OutputFormat::JsonLines => {
            json::write(&Envelope::new(AlertContent { alert: view }))?;
        }
        OutputFormat::Table => {
            let chain = alert
                .chain
                .map(|c| format!(" hos {}", c.display_name()))
                .unwrap_or_default();
            println!(
                "la til varsel {id}: {} under {}{chain}",
                view.name,
                alert.kind.amount(alert.threshold)
            );
            match (view.best_price, view.best_chain) {
                (Some(price), Some(chain)) => println!(
                    "nå: {} hos {}{}",
                    alert.kind.amount(price),
                    chain.display_name(),
                    if view.below {
                        " – allerede under grensen"
                    } else {
                        ""
                    }
                ),
                _ => println!("ingen fersk pris nå"),
            }
            println!(
                "varselet sjekkes etter hver henting (`databrus planlegg installer` henter daglig)"
            );
        }
    }
    Ok(ExitStatus::Ok)
}

fn list(ctx: &Context) -> Result<ExitStatus, AppError> {
    let catalog = ctx.catalog()?;
    let db = ctx.open_database()?;
    let alerts = db.alerts()?;
    let rows = super::current_rows(&db, &catalog, ctx)?;
    let views: Vec<AlertView<'_>> = alerts.iter().map(|a| view(a, &catalog, &rows)).collect();

    match ctx.output_format() {
        OutputFormat::Json => json::write(&Envelope::new(AlertsContent { alerts: views }))?,
        OutputFormat::JsonLines => json::write_lines(&Header::new(), &views)?,
        OutputFormat::Table if views.is_empty() => {
            eprintln!("ingen prisvarsler – legg til med `databrus overvak legg-til`");
        }
        OutputFormat::Table => {
            let mut t = table::new(&["Id", "Produkt", "Kjede", "Grense", "Beste nå", "Status"]);
            for v in &views {
                let best = match (v.best_price, v.best_chain) {
                    (Some(price), Some(chain)) => {
                        format!(
                            "{} hos {}",
                            v.alert.kind.amount(price),
                            chain.display_name()
                        )
                    }
                    _ => "–".to_owned(),
                };
                t.add_row(vec![
                    v.alert.id.to_string(),
                    v.name.clone(),
                    v.alert.chain.map_or("alle", Chain::display_name).to_owned(),
                    format!("under {}", v.alert.kind.amount(v.alert.threshold)),
                    best,
                    if v.below { "UNDER" } else { "" }.to_owned(),
                ]);
            }
            table::write(&t)?;
        }
    }
    Ok(ExitStatus::Ok)
}

fn remove(id: i64, ctx: &Context) -> Result<ExitStatus, AppError> {
    let mut db = ctx.open_database()?;
    if !db.remove_alert(id)? {
        return Err(AppError::Usage(format!(
            "fant ikke varsel {id} – se `databrus overvak liste`"
        )));
    }
    if ctx.output_format() == OutputFormat::Table {
        println!("fjernet varsel {id}");
    }
    Ok(ExitStatus::Ok)
}

fn view<'a>(alert: &'a Alert, catalog: &Catalog, rows: &[SearchResult]) -> AlertView<'a> {
    let name = catalog.find(&alert.product).map_or_else(
        || alert.product.0.clone(),
        |p| format!("{} {}", p.name, format::liters(p.volume)),
    );
    let best = alert.best(rows);
    AlertView {
        alert,
        name,
        best_price: best.map(|r| alert.kind.price(r)),
        best_chain: best.map(|r| r.chain),
        below: best.is_some_and(|r| alert.triggered_by(r)),
    }
}
