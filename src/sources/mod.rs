//! Price sources (SPEC §4) and fetch orchestration (SPEC §7.3–7.4).
//!
//! Every source implements [`Source`]. [`refresh`] decides which sources need fetching
//! (TTL and the 15-minute floor), fetches them concurrently, stores the results and
//! prints warnings on stderr for sources that fail – without stopping the others.

pub mod coop;
pub mod http;
pub mod kassalapp;
pub mod oda;
pub mod rema;

use std::sync::Arc;

use async_trait::async_trait;
use jiff::Timestamp;
use owo_colors::OwoColorize;
use serde::Serialize;
use tokio::task::JoinSet;

use crate::catalog::Catalog;
use crate::catalog::matching::Matcher;
use crate::config::{Config, Fetching};
use crate::db::{Database, FetchLog, PriceObservation};
use crate::error::AppError;
use crate::model::{self, Chain, RawListing, SourceId};

pub use http::Http;

/// A source failure. The messages are Norwegian, since they are shown as warnings.
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("ikke implementert ennå")]
    NotImplemented,

    #[error("mangler API-nøkkel")]
    MissingApiKey,

    #[error("API-nøkkelen ble avvist (HTTP {status})")]
    RejectedKey { status: u16 },

    #[error("ugyldig URL: {0}")]
    InvalidUrl(String),

    #[error("HTTP {status}")]
    Http { status: u16 },

    #[error("nettverksfeil: {0}")]
    Network(#[from] reqwest::Error),

    #[error("uventet svarformat: {0}")]
    SchemaChange(String),

    #[error("avbrutt")]
    Cancelled,
}

/// What a source gets when it fetches.
#[derive(Debug)]
pub struct FetchContext {
    pub http: Http,
    pub api_key: Option<String>,
    /// Known EANs from the catalog, normalized to 14 digits, for per-product lookups.
    pub gtins: Vec<String>,
}

/// A price source. A fetch always covers the source's whole energy drink category, not a
/// single search – searching happens locally, so the TTL applies per source.
#[async_trait]
pub trait Source: Send + Sync {
    fn id(&self) -> SourceId;
    fn chains(&self) -> &'static [Chain];
    async fn fetch(&self, ctx: &FetchContext) -> Result<Vec<RawListing>, SourceError>;
}

pub fn all() -> Vec<Box<dyn Source>> {
    vec![
        Box::new(kassalapp::Kassalapp::from_env()),
        Box::new(oda::Oda),
        Box::new(rema::Rema),
        Box::new(coop::Coop),
    ]
}

/// The sources enabled in the config, optionally limited to `only`.
pub fn enabled(config: &Config, only: &[SourceId]) -> Vec<Box<dyn Source>> {
    all()
        .into_iter()
        .filter(|s| {
            config.sources.is_enabled(s.id()) && (only.is_empty() || only.contains(&s.id()))
        })
        .collect()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FetchMode {
    /// Fetch even when the data is fresh (`--oppdater`, `oppdater`). The floor still applies.
    pub force: bool,
    /// Never touch the network (`--frakoblet`).
    pub offline: bool,
    /// No informational messages, only warnings and errors.
    pub quiet: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Fetch,
    Fresh,
    Offline,
    TooSoon { wait_minutes: u32 },
}

/// Decides whether a source should be fetched now (SPEC §7.3–7.4).
pub fn decide_fetch(
    mode: FetchMode,
    last_attempt: Option<Timestamp>,
    last_success: Option<Timestamp>,
    now: Timestamp,
    fetching: &Fetching,
) -> Decision {
    if mode.offline {
        return Decision::Offline;
    }
    let floor = i64::from(fetching.min_interval_minutes);
    if let Some(attempt) = last_attempt {
        let elapsed = minutes_between(attempt, now);
        if elapsed < floor {
            return Decision::TooSoon {
                wait_minutes: u32::try_from(floor - elapsed).unwrap_or(0),
            };
        }
    }
    if !mode.force
        && let Some(success) = last_success
        && minutes_between(success, now) < i64::from(fetching.ttl_hours) * 60
    {
        return Decision::Fresh;
    }
    Decision::Fetch
}

fn minutes_between(from: Timestamp, to: Timestamp) -> i64 {
    to.duration_since(from).as_secs() / 60
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SourceState {
    #[serde(rename = "ok")]
    Ok,
    #[serde(rename = "feilet")]
    Failed,
    #[serde(rename = "aldri_hentet")]
    NeverFetched,
}

/// The status of one source, as shown under `kilder` in JSON (SPEC §9.1).
#[derive(Debug, Clone, Serialize)]
pub struct SourceStatus {
    pub id: SourceId,
    pub status: SourceState,
    /// The last successful fetch – the age of the data.
    #[serde(rename = "hentet")]
    pub fetched: Option<Timestamp>,
    #[serde(rename = "feil", skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Listings fetched in this run.
    #[serde(skip)]
    pub new_listings: Option<usize>,
    /// How many of them matched a catalog product.
    #[serde(skip)]
    pub matched: Option<usize>,
}

impl SourceStatus {
    fn from_log(id: SourceId, last: Option<&FetchLog>, last_success: Option<Timestamp>) -> Self {
        let (status, error) = match last {
            None => (SourceState::NeverFetched, None),
            Some(log) if log.ok => (SourceState::Ok, None),
            Some(log) => (SourceState::Failed, log.error_message.clone()),
        };
        Self {
            id,
            status,
            fetched: last_success,
            error,
            new_listings: None,
            matched: None,
        }
    }
}

/// Fetches from the sources that need it and returns the status of every enabled source.
pub async fn refresh(
    db: &mut Database,
    config: &Config,
    catalog: &Catalog,
    mode: FetchMode,
    only: &[SourceId],
) -> Result<Vec<SourceStatus>, AppError> {
    let now = model::now();
    let mut statuses = Vec::new();
    let mut to_fetch = Vec::new();

    for source in enabled(config, only) {
        let id = source.id();
        let last = db.last_fetch(id)?;
        let last_success = db.last_successful_fetch(id)?;
        let decision = decide_fetch(
            mode,
            last.as_ref().map(|l| l.started),
            last_success,
            now,
            &config.fetching,
        );
        if decision == Decision::Fetch {
            to_fetch.push(source);
            continue;
        }
        if let Decision::TooSoon { wait_minutes } = decision
            && mode.force
            && !mode.quiet
        {
            anstream::eprintln!(
                "{} {}: hentet nylig – neste henting tidligst om {wait_minutes} min",
                "info:".cyan().bold(),
                id.display_name()
            );
        }
        let status = SourceStatus::from_log(id, last.as_ref(), last_success);
        if let Some(error) = &status.error {
            warn(id, &format!("feilet ved siste henting ({error})"));
        }
        statuses.push(status);
    }

    if !to_fetch.is_empty() {
        let http = Http::new().map_err(|error| AppError::Network(error.to_string()))?;
        let ctx = Arc::new(FetchContext {
            http,
            api_key: config.api_key(),
            gtins: catalog.all_gtins(),
        });

        let mut tasks = JoinSet::new();
        for source in to_fetch {
            let ctx = Arc::clone(&ctx);
            tasks.spawn(async move {
                let started = model::now();
                let result = source.fetch(&ctx).await;
                (source.id(), started, result)
            });
        }
        let mut finished = Vec::new();
        while let Some(result) = tasks.join_next().await {
            match result {
                Ok(done) => finished.push(done),
                Err(error) => tracing::error!("henteoppgave stoppet uventet: {error}"),
            }
        }
        finished.sort_by_key(|(id, ..)| *id);

        let matcher = Matcher::build(catalog);
        for (id, started, result) in finished {
            let done = model::now();
            match result {
                Ok(listings) => {
                    let (count, matched) = store_listings(db, catalog, &matcher, &listings, done)?;
                    db.log_fetch(id, started, done, Ok(count))?;
                    statuses.push(SourceStatus {
                        id,
                        status: SourceState::Ok,
                        fetched: Some(done),
                        error: None,
                        new_listings: Some(count),
                        matched: Some(matched),
                    });
                }
                Err(error) => {
                    let message = error.to_string();
                    db.log_fetch(id, started, done, Err(&message))?;
                    warn(id, &message);
                    if matches!(
                        error,
                        SourceError::MissingApiKey | SourceError::RejectedKey { .. }
                    ) {
                        anstream::eprintln!(
                            "  Hent en gratis nøkkel på https://kassal.app/api og sett {},\n  \
                             eller kjør: databrus konfig sett kilder.kassalapp.api_nokkel <NØKKEL>",
                            crate::config::ENV_API_KEY
                        );
                    }
                    statuses.push(SourceStatus {
                        id,
                        status: SourceState::Failed,
                        fetched: db.last_successful_fetch(id)?,
                        error: Some(message),
                        new_listings: None,
                        matched: None,
                    });
                }
            }
        }
    }

    statuses.sort_by_key(|s| s.id);
    Ok(statuses)
}

/// Stores a source's listings with change-only history. Returns (count, matched).
fn store_listings(
    db: &mut Database,
    catalog: &Catalog,
    matcher: &Matcher,
    listings: &[RawListing],
    now: Timestamp,
) -> Result<(usize, usize), AppError> {
    db.sync_catalog(catalog)?;
    let mut matched = 0;
    for listing in listings {
        let found = matcher.find(listing);
        matched += usize::from(found.is_some());
        // Stores use the same EAN for a single can and a tray (Engrossnett), and
        // sometimes a pack EAN for one can. The listing's name therefore decides the
        // pack size.
        if let Some(m) = &found
            && m.pack_size != listing.pack_size
        {
            tracing::debug!(
                "{}: EAN sier {} stk, navnet «{}» sier {} – bruker navnet",
                listing.chain.slug(),
                m.pack_size,
                listing.raw_name,
                listing.pack_size
            );
        }
        let id = db.save_listing(listing, found.as_ref(), now)?;
        // The observation time is when the source saw the price, not when we fetched it
        // (SPEC §4.5). A timestamp in the future is not trusted.
        let observed = listing.source_timestamp.map_or(now, |t| t.min(now));
        // The sanity check (SPEC §5.5) is wired in once prices are computed on storage.
        db.record_price(id, &PriceObservation::from_listing(listing), observed)?;
    }
    Ok((listings.len(), matched))
}

fn warn(id: SourceId, message: &str) {
    anstream::eprintln!(
        "{} {}: {message}",
        "advarsel:".yellow().bold(),
        id.display_name()
    );
}

#[cfg(test)]
mod tests {
    use jiff::ToSpan;

    use super::*;

    fn minutes_ago(minutes: i64) -> Option<Timestamp> {
        Some(now() - minutes.minutes())
    }

    fn now() -> Timestamp {
        "2026-09-29T12:00:00Z".parse().unwrap()
    }

    fn decide(mode: FetchMode, attempt: Option<Timestamp>, success: Option<Timestamp>) -> Decision {
        decide_fetch(mode, attempt, success, now(), &Fetching::default())
    }

    #[test]
    fn never_fetched_is_fetched() {
        assert_eq!(decide(FetchMode::default(), None, None), Decision::Fetch);
    }

    #[test]
    fn offline_never_fetches() {
        let mode = FetchMode {
            offline: true,
            force: true,
            ..FetchMode::default()
        };
        assert_eq!(decide(mode, None, None), Decision::Offline);
    }

    #[test]
    fn fresh_data_is_not_fetched() {
        assert_eq!(
            decide(FetchMode::default(), minutes_ago(60), minutes_ago(60)),
            Decision::Fresh
        );
        assert_eq!(
            decide(
                FetchMode::default(),
                minutes_ago(7 * 60),
                minutes_ago(7 * 60)
            ),
            Decision::Fetch
        );
    }

    #[test]
    fn force_skips_the_ttl_but_not_the_floor() {
        let force = FetchMode {
            force: true,
            ..FetchMode::default()
        };
        assert_eq!(
            decide(force, minutes_ago(60), minutes_ago(60)),
            Decision::Fetch
        );
        assert_eq!(
            decide(force, minutes_ago(5), minutes_ago(5)),
            Decision::TooSoon { wait_minutes: 10 }
        );
    }

    #[test]
    fn a_failed_attempt_counts_toward_the_floor() {
        assert_eq!(
            decide(FetchMode::default(), minutes_ago(3), None),
            Decision::TooSoon { wait_minutes: 12 }
        );
    }
}
