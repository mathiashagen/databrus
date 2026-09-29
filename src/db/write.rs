//! Writing: products, listings, change-only price history and the fetch log.

use jiff::Timestamp;
use rusqlite::{Connection, OptionalExtension, params};

use super::{Database, error};
use crate::catalog::Catalog;
use crate::catalog::matching::Match;
use crate::error::AppError;
use crate::model::{MemberPrice, OfferInfo, Ore, Product, RawListing, SourceId};

/// What is stored per price observation.
#[derive(Debug, Clone, PartialEq)]
pub struct PriceObservation {
    pub shelf_price: Ore,
    pub member_price: Option<MemberPrice>,
    pub offer: Option<OfferInfo>,
    pub available: Option<bool>,
    pub suspicious: bool,
}

impl PriceObservation {
    pub fn from_listing(listing: &RawListing) -> Self {
        Self {
            shelf_price: listing.shelf_price,
            member_price: listing.member_price,
            offer: listing.offer.clone(),
            available: listing.available,
            suspicious: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// Same price as last time – only `last_seen` was updated.
    Unchanged,
    /// New price – a new interval was added.
    NewInterval(i64),
}

/// The fields that decide whether an observation is a change.
#[derive(Debug, PartialEq)]
struct PriceKey {
    shelf_price: i64,
    member_price: Option<i64>,
    membership_program: Option<String>,
    offer_json: Option<String>,
    available: Option<bool>,
}

impl Database {
    pub fn save_product(&self, product: &Product, ad_hoc: bool) -> Result<(), AppError> {
        write_product(&self.conn, product, ad_hoc).map_err(|e| error(&self.path, e))
    }

    /// Makes sure every catalog product exists in the database (listings point to them).
    pub fn sync_catalog(&mut self, catalog: &Catalog) -> Result<(), AppError> {
        let tx = self.conn.transaction().map_err(|e| error(&self.path, e))?;
        for product in &catalog.products {
            write_product(&tx, product, false).map_err(|e| error(&self.path, e))?;
        }
        tx.commit().map_err(|e| error(&self.path, e))
    }

    /// Inserts or updates a listing and returns its id.
    pub fn save_listing(
        &self,
        listing: &RawListing,
        matched: Option<&Match>,
        now: Timestamp,
    ) -> Result<i64, AppError> {
        self.conn
            .query_row(
                "INSERT INTO listing
                     (source, chain, source_product_id, gtin, product_id, pack_size, raw_name,
                      verified, first_seen, last_seen)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
                 ON CONFLICT (source, chain, source_product_id) DO UPDATE SET
                     gtin = excluded.gtin, product_id = excluded.product_id,
                     pack_size = excluded.pack_size, raw_name = excluded.raw_name,
                     verified = excluded.verified, last_seen = excluded.last_seen
                 RETURNING id",
                params![
                    listing.source.slug(),
                    listing.chain.slug(),
                    listing.source_product_id,
                    listing.gtin,
                    matched.map(|m| &m.product.0),
                    // The pack size belongs to the listing, not the EAN (SPEC §6.2).
                    listing.pack_size.max(1),
                    listing.raw_name,
                    matched.is_some_and(|m| m.verified),
                    now.as_second(),
                ],
                |row| row.get(0),
            )
            .map_err(|e| error(&self.path, e))
    }

    /// Records a price observation made at time `observed`. If it equals the newest
    /// interval, only `last_seen` is updated (never backwards in time); otherwise a new
    /// interval starts (SPEC §7.2).
    pub fn record_price(
        &mut self,
        listing_id: i64,
        observation: &PriceObservation,
        observed: Timestamp,
    ) -> Result<Change, AppError> {
        let offer_json = observation
            .offer
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let new = PriceKey {
            shelf_price: observation.shelf_price.0,
            member_price: observation.member_price.map(|m| m.price.0),
            membership_program: observation
                .member_price
                .map(|m| m.program.slug().to_owned()),
            offer_json,
            available: observation.available,
        };

        let tx = self.conn.transaction().map_err(|e| error(&self.path, e))?;
        let latest = tx
            .query_row(
                "SELECT id, shelf_price_ore, member_price_ore, membership_program, offer_json,
                        available
                 FROM price_interval WHERE listing_id = ?1
                 ORDER BY valid_from DESC, id DESC LIMIT 1",
                [listing_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        PriceKey {
                            shelf_price: row.get(1)?,
                            member_price: row.get(2)?,
                            membership_program: row.get(3)?,
                            offer_json: row.get(4)?,
                            available: row.get(5)?,
                        },
                    ))
                },
            )
            .optional()
            .map_err(|e| error(&self.path, e))?;

        let change = match latest {
            Some((id, previous)) if previous == new => {
                tx.execute(
                    "UPDATE price_interval SET last_seen = MAX(last_seen, ?1) WHERE id = ?2",
                    params![observed.as_second(), id],
                )
                .map_err(|e| error(&self.path, e))?;
                Change::Unchanged
            }
            _ => {
                let id = tx
                    .query_row(
                        "INSERT INTO price_interval
                             (listing_id, shelf_price_ore, member_price_ore, membership_program,
                              offer_json, available, suspicious, valid_from, last_seen)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
                         RETURNING id",
                        params![
                            listing_id,
                            new.shelf_price,
                            new.member_price,
                            new.membership_program,
                            new.offer_json,
                            new.available,
                            observation.suspicious,
                            observed.as_second(),
                        ],
                        |row| row.get(0),
                    )
                    .map_err(|e| error(&self.path, e))?;
                Change::NewInterval(id)
            }
        };
        tx.commit().map_err(|e| error(&self.path, e))?;
        Ok(change)
    }

    /// Logs a fetch attempt. `result` is the listing count, or the error message.
    pub fn log_fetch(
        &self,
        source: SourceId,
        started: Timestamp,
        finished: Timestamp,
        result: Result<usize, &str>,
    ) -> Result<(), AppError> {
        let (status, error_message, count) = match result {
            Ok(count) => ("ok", None, i64::try_from(count).ok()),
            Err(message) => ("failed", Some(message), None),
        };
        self.conn
            .execute(
                "INSERT INTO fetch_log (source, started, finished, status, error_message, listing_count)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    source.slug(),
                    started.as_second(),
                    finished.as_second(),
                    status,
                    error_message,
                    count,
                ],
            )
            .map_err(|e| error(&self.path, e))?;
        Ok(())
    }
}

fn write_product(conn: &Connection, product: &Product, ad_hoc: bool) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO product
             (id, name, brand, line, flavor, sugar_free, volume_ml, container, store_brand, ad_hoc)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT (id) DO UPDATE SET
             name = excluded.name, brand = excluded.brand, line = excluded.line,
             flavor = excluded.flavor, sugar_free = excluded.sugar_free,
             volume_ml = excluded.volume_ml, container = excluded.container,
             store_brand = excluded.store_brand, ad_hoc = excluded.ad_hoc",
        params![
            product.id.0,
            product.name,
            product.brand,
            product.line,
            product.flavor,
            product.sugar_free,
            product.volume.get(),
            product.container.slug(),
            product.store_brand,
            ad_hoc,
        ],
    )?;
    Ok(())
}
