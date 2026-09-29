//! `databrus butikker` – known chains, which sources cover them and how fresh the data is.

use jiff::Timestamp;
use serde::Serialize;

use crate::Context;
use crate::error::{AppError, ExitStatus};
use crate::model::{self, Chain, SourceId};
use crate::output::json::{self, Envelope, Header};
use crate::output::{OutputFormat, format, table};
use crate::sources;

#[derive(Debug, Serialize)]
struct Store {
    #[serde(rename = "kjede")]
    chain: Chain,
    #[serde(rename = "navn")]
    name: &'static str,
    #[serde(rename = "gruppe")]
    group: Option<&'static str>,
    #[serde(rename = "kilder")]
    sources: Vec<SourceId>,
    /// The last successful fetch from a source covering the chain.
    #[serde(rename = "sist_hentet")]
    last_fetched: Option<Timestamp>,
    /// When the chain's newest price was observed at the source. Can be much older than
    /// `last_fetched` (SPEC §4.5).
    #[serde(rename = "nyeste_pris")]
    newest_price: Option<Timestamp>,
}

#[derive(Debug, Serialize)]
struct StoresContent {
    #[serde(rename = "butikker")]
    stores: Vec<Store>,
}

pub fn run(ctx: &Context) -> Result<ExitStatus, AppError> {
    let db = ctx.open_database()?;
    let enabled = sources::enabled(&ctx.config, &[]);
    let mut last_fetched = Vec::new();
    for source in &enabled {
        last_fetched.push((source.id(), db.last_successful_fetch(source.id())?));
    }
    let newest_price = db.newest_price_per_chain()?;

    let stores: Vec<Store> = Chain::ALL
        .into_iter()
        .map(|chain| {
            let covering: Vec<_> = enabled
                .iter()
                .filter(|s| s.chains().contains(&chain))
                .map(|s| s.id())
                .collect();
            let last = last_fetched
                .iter()
                .filter(|(id, _)| covering.contains(id))
                .filter_map(|(_, t)| *t)
                .max();
            Store {
                chain,
                name: chain.display_name(),
                group: chain.group(),
                sources: covering,
                last_fetched: last,
                newest_price: newest_price.get(&chain).copied(),
            }
        })
        .collect();

    match ctx.output_format() {
        OutputFormat::Json => json::write(&Envelope::new(StoresContent { stores }))?,
        OutputFormat::JsonLines => json::write_lines(&Header::new(), &stores)?,
        OutputFormat::Table => {
            let now = model::now();
            let ago = |t: Option<Timestamp>, none: &str| {
                t.map_or_else(
                    || none.to_owned(),
                    |t| {
                        let hours = u32::try_from(now.duration_since(t).as_hours()).unwrap_or(0);
                        format!("{} siden", format::age(hours))
                    },
                )
            };
            let mut t = table::new(&[
                "Kjede",
                "Valg",
                "Gruppe",
                "Kilder",
                "Sist hentet",
                "Nyeste pris",
            ]);
            for s in &stores {
                let sources: Vec<_> = s.sources.iter().map(|id| id.slug()).collect();
                t.add_row(vec![
                    s.name.to_owned(),
                    s.chain.slug().to_owned(),
                    s.group.unwrap_or("–").to_owned(),
                    sources.join(", "),
                    ago(s.last_fetched, "aldri hentet"),
                    ago(s.newest_price, "–"),
                ]);
            }
            table::write(&t)?;
        }
    }
    Ok(ExitStatus::Ok)
}
