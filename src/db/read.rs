//! Reading. The queries for history and export come in M2.

use std::collections::HashMap;

use jiff::Timestamp;
use rusqlite::types::Type;
use rusqlite::{OptionalExtension, Row};
use serde::Serialize;

use super::{Database, error};
use crate::error::AppError;
use crate::model::{Chain, MemberPrice, MembershipProgram, OfferInfo, Ore, ProductId, SourceId};

/// One row from the fetch log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchLog {
    pub source: SourceId,
    pub started: Timestamp,
    pub finished: Timestamp,
    pub ok: bool,
    pub error_message: Option<String>,
    pub listing_count: Option<i64>,
}

/// The current price of a listing that matched the catalog: its newest price interval.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredPrice {
    pub listing_id: i64,
    pub source: SourceId,
    pub chain: Chain,
    pub product: ProductId,
    /// Containers in the sellable unit.
    pub pack_size: u32,
    pub verified: bool,
    pub shelf_price: Ore,
    pub member_price: Option<MemberPrice>,
    pub offer: Option<OfferInfo>,
    pub available: Option<bool>,
    pub suspicious: bool,
    /// When the source last saw the price.
    pub last_seen: Timestamp,
}

/// A listing that did not match any catalog product (`produkter --ukjente`). Serialized
/// with Norwegian keys, since it is JSON output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UnmatchedListing {
    #[serde(rename = "kilde")]
    pub source: String,
    #[serde(rename = "kjede")]
    pub chain: String,
    #[serde(rename = "kilde_produkt_id")]
    pub source_product_id: String,
    #[serde(rename = "raanavn")]
    pub raw_name: String,
    pub gtin: Option<String>,
    #[serde(rename = "sist_sett")]
    pub last_seen: Timestamp,
}

impl Database {
    pub fn has_price_data(&self) -> Result<bool, AppError> {
        self.conn
            .query_row("SELECT EXISTS (SELECT 1 FROM price_interval)", [], |row| {
                row.get(0)
            })
            .map_err(|e| error(&self.path, e))
    }

    /// The newest price of every listing that matched a catalog product. Rows with an
    /// unknown source, chain or membership program (e.g. from a newer version) are
    /// skipped, and an offer that cannot be parsed is ignored with a log line.
    pub fn latest_prices(&self) -> Result<Vec<StoredPrice>, AppError> {
        let mut query = self
            .conn
            .prepare(
                "SELECT l.id, l.source, l.chain, l.product_id, l.pack_size, l.verified,
                        p.shelf_price_ore, p.member_price_ore, p.membership_program,
                        p.offer_json, p.available, p.suspicious, p.last_seen
                 FROM listing l
                 JOIN price_interval p ON p.id = (
                     SELECT id FROM price_interval WHERE listing_id = l.id
                     ORDER BY valid_from DESC, id DESC LIMIT 1)
                 WHERE l.product_id IS NOT NULL",
            )
            .map_err(|e| error(&self.path, e))?;
        let rows = query
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, u32>(4)?,
                    row.get::<_, bool>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, Option<i64>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, Option<bool>>(10)?,
                    row.get::<_, bool>(11)?,
                    timestamp(row, 12)?,
                ))
            })
            .map_err(|e| error(&self.path, e))?;

        let mut prices = Vec::new();
        for row in rows {
            let (
                id,
                source,
                chain,
                product,
                pack_size,
                verified,
                shelf_price,
                member_price,
                program,
                offer,
                available,
                suspicious,
                last_seen,
            ) = row.map_err(|e| error(&self.path, e))?;
            let (Some(source), Some(chain)) =
                (SourceId::from_slug(&source), Chain::from_slug(&chain))
            else {
                continue;
            };
            let member_price = match (
                member_price,
                program.as_deref().map(MembershipProgram::from_slug),
            ) {
                (Some(price), Some(Some(program))) => Some(MemberPrice {
                    price: Ore(price),
                    program,
                }),
                _ => None,
            };
            let offer = offer.and_then(|json| {
                serde_json::from_str(&json)
                    .inspect_err(|e| tracing::debug!("oppføring {id}: ugyldig tilbud ({e})"))
                    .ok()
            });
            prices.push(StoredPrice {
                listing_id: id,
                source,
                chain,
                product: ProductId(product),
                pack_size: pack_size.max(1),
                verified,
                shelf_price: Ore(shelf_price),
                member_price,
                offer,
                available,
                suspicious,
                last_seen,
            });
        }
        Ok(prices)
    }

    pub fn price_interval_count(&self) -> Result<i64, AppError> {
        self.conn
            .query_row("SELECT COUNT(*) FROM price_interval", [], |row| row.get(0))
            .map_err(|e| error(&self.path, e))
    }

    /// When the newest price per chain was observed. Rows with an unknown chain slug are
    /// ignored.
    pub fn newest_price_per_chain(&self) -> Result<HashMap<Chain, Timestamp>, AppError> {
        let mut query = self
            .conn
            .prepare(
                "SELECT l.chain, MAX(p.last_seen)
                 FROM price_interval p JOIN listing l ON l.id = p.listing_id
                 GROUP BY l.chain",
            )
            .map_err(|e| error(&self.path, e))?;
        let rows = query
            .query_map([], |row| Ok((row.get::<_, String>(0)?, timestamp(row, 1)?)))
            .map_err(|e| error(&self.path, e))?;
        let mut newest = HashMap::new();
        for row in rows {
            let (slug, time) = row.map_err(|e| error(&self.path, e))?;
            if let Some(chain) = Chain::from_slug(&slug) {
                newest.insert(chain, time);
            }
        }
        Ok(newest)
    }

    /// When the price of a listing was last observed.
    pub fn price_last_seen(&self, listing_id: i64) -> Result<Option<Timestamp>, AppError> {
        self.conn
            .query_row(
                "SELECT MAX(last_seen) FROM price_interval WHERE listing_id = ?1",
                [listing_id],
                |row| row.get::<_, Option<i64>>(0),
            )
            .map_err(|e| error(&self.path, e))?
            .map(|seconds| Timestamp::from_second(seconds).map_err(|e| error(&self.path, e)))
            .transpose()
    }

    /// The latest fetch attempt for a source, successful or not.
    pub fn last_fetch(&self, source: SourceId) -> Result<Option<FetchLog>, AppError> {
        self.conn
            .query_row(
                "SELECT started, finished, status, error_message, listing_count
                 FROM fetch_log WHERE source = ?1
                 ORDER BY started DESC, id DESC LIMIT 1",
                [source.slug()],
                |row| {
                    Ok(FetchLog {
                        source,
                        started: timestamp(row, 0)?,
                        finished: timestamp(row, 1)?,
                        ok: row.get::<_, String>(2)? == "ok",
                        error_message: row.get(3)?,
                        listing_count: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(|e| error(&self.path, e))
    }

    pub fn last_successful_fetch(&self, source: SourceId) -> Result<Option<Timestamp>, AppError> {
        self.conn
            .query_row(
                "SELECT finished FROM fetch_log WHERE source = ?1 AND status = 'ok'
                 ORDER BY finished DESC LIMIT 1",
                [source.slug()],
                |row| timestamp(row, 0),
            )
            .optional()
            .map_err(|e| error(&self.path, e))
    }

    pub fn unmatched_listings(&self) -> Result<Vec<UnmatchedListing>, AppError> {
        let mut query = self
            .conn
            .prepare(
                "SELECT source, chain, source_product_id, raw_name, gtin, last_seen
                 FROM listing WHERE product_id IS NULL
                 ORDER BY raw_name, chain",
            )
            .map_err(|e| error(&self.path, e))?;
        let rows = query
            .query_map([], |row| {
                Ok(UnmatchedListing {
                    source: row.get(0)?,
                    chain: row.get(1)?,
                    source_product_id: row.get(2)?,
                    raw_name: row.get(3)?,
                    gtin: row.get(4)?,
                    last_seen: timestamp(row, 5)?,
                })
            })
            .map_err(|e| error(&self.path, e))?;
        rows.collect::<Result<_, _>>()
            .map_err(|e| error(&self.path, e))
    }
}

fn timestamp(row: &Row<'_>, column: usize) -> rusqlite::Result<Timestamp> {
    let seconds: i64 = row.get(column)?;
    Timestamp::from_second(seconds)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(column, Type::Integer, Box::new(e)))
}
