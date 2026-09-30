//! Reading.

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
    /// The price interval this price comes from.
    pub interval_id: i64,
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

/// One price interval of a listing: the price was seen from `valid_from` until
/// `price.last_seen`.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoricPrice {
    pub price: StoredPrice,
    pub valid_from: Timestamp,
}

/// A row as it comes from the database, before the slugs are parsed.
struct PriceRow {
    id: i64,
    source: String,
    chain: String,
    product: String,
    pack_size: u32,
    verified: bool,
    shelf_price: i64,
    member_price: Option<i64>,
    program: Option<String>,
    offer: Option<String>,
    available: Option<bool>,
    suspicious: bool,
    last_seen: Timestamp,
    valid_from: Timestamp,
    interval_id: i64,
}

impl PriceRow {
    fn into_price(self) -> Option<HistoricPrice> {
        let source = SourceId::from_slug(&self.source)?;
        let chain = Chain::from_slug(&self.chain)?;
        let member_price = match (
            self.member_price,
            self.program.as_deref().map(MembershipProgram::from_slug),
        ) {
            (Some(price), Some(Some(program))) => Some(MemberPrice {
                price: Ore(price),
                program,
            }),
            _ => None,
        };
        let id = self.id;
        let offer = self.offer.and_then(|json| {
            serde_json::from_str(&json)
                .inspect_err(|e| tracing::debug!("oppføring {id}: ugyldig tilbud ({e})"))
                .ok()
        });
        Some(HistoricPrice {
            price: StoredPrice {
                listing_id: id,
                interval_id: self.interval_id,
                source,
                chain,
                product: ProductId(self.product),
                pack_size: self.pack_size.max(1),
                verified: self.verified,
                shelf_price: Ore(self.shelf_price),
                member_price,
                offer,
                available: self.available,
                suspicious: self.suspicious,
                last_seen: self.last_seen,
            },
            valid_from: self.valid_from,
        })
    }
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
        let prices = self.read_prices(
            "JOIN price_interval p ON p.id = (
                 SELECT id FROM price_interval WHERE listing_id = l.id
                 ORDER BY valid_from DESC, id DESC LIMIT 1)
             WHERE l.product_id IS NOT NULL",
        )?;
        Ok(prices.into_iter().map(|h| h.price).collect())
    }

    /// Every price interval of every listing that matched a catalog product, ordered by
    /// listing and then time. Skips the same rows as [`Self::latest_prices`].
    pub fn price_history(&self) -> Result<Vec<HistoricPrice>, AppError> {
        self.read_prices(
            "JOIN price_interval p ON p.listing_id = l.id
             WHERE l.product_id IS NOT NULL
             ORDER BY l.id, p.valid_from, p.id",
        )
    }

    /// Price intervals joined with their listing. `rest` joins `price_interval p` to
    /// `listing l` and filters.
    fn read_prices(&self, rest: &str) -> Result<Vec<HistoricPrice>, AppError> {
        let sql = format!(
            "SELECT l.id, l.source, l.chain, l.product_id, l.pack_size, l.verified,
                    p.shelf_price_ore, p.member_price_ore, p.membership_program,
                    p.offer_json, p.available, p.suspicious, p.last_seen, p.valid_from,
                    p.id
             FROM listing l {rest}"
        );
        let mut query = self.conn.prepare(&sql).map_err(|e| error(&self.path, e))?;
        let rows = query
            .query_map([], |row| {
                Ok(PriceRow {
                    id: row.get(0)?,
                    source: row.get(1)?,
                    chain: row.get(2)?,
                    product: row.get(3)?,
                    pack_size: row.get(4)?,
                    verified: row.get(5)?,
                    shelf_price: row.get(6)?,
                    member_price: row.get(7)?,
                    program: row.get(8)?,
                    offer: row.get(9)?,
                    available: row.get(10)?,
                    suspicious: row.get(11)?,
                    last_seen: timestamp(row, 12)?,
                    valid_from: timestamp(row, 13)?,
                    interval_id: row.get(14)?,
                })
            })
            .map_err(|e| error(&self.path, e))?;

        let mut prices = Vec::new();
        for row in rows {
            if let Some(price) = row.map_err(|e| error(&self.path, e))?.into_price() {
                prices.push(price);
            }
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

pub(super) fn timestamp(row: &Row<'_>, column: usize) -> rusqlite::Result<Timestamp> {
    let seconds: i64 = row.get(column)?;
    Timestamp::from_second(seconds)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(column, Type::Integer, Box::new(e)))
}
