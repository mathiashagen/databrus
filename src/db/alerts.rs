//! Price alerts (SPEC §7.7).

use jiff::Timestamp;
use rusqlite::params;

use super::read::timestamp;
use super::{Database, error};
use crate::alerts::{Alert, NewAlert, ThresholdKind};
use crate::error::AppError;
use crate::model::{Chain, Ore, ProductId};

impl Database {
    /// Stores a new alert and returns its id.
    pub fn add_alert(&mut self, alert: &NewAlert, created: Timestamp) -> Result<i64, AppError> {
        self.conn
            .query_row(
                "INSERT INTO alert (product_id, chain, threshold_ore, threshold_kind, created)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 RETURNING id",
                params![
                    alert.product.0,
                    alert.chain.map(Chain::slug),
                    alert.threshold.0,
                    alert.kind.db_value(),
                    created.as_second(),
                ],
                |row| row.get(0),
            )
            .map_err(|e| error(&self.path, e))
    }

    /// Removes an alert. Returns `false` if there was none with that id.
    pub fn remove_alert(&mut self, id: i64) -> Result<bool, AppError> {
        let removed = self
            .conn
            .execute("DELETE FROM alert WHERE id = ?1", [id])
            .map_err(|e| error(&self.path, e))?;
        Ok(removed > 0)
    }

    /// All alerts, oldest first. Rows with an unknown chain or kind (e.g. from a newer
    /// version) are skipped.
    pub fn alerts(&self) -> Result<Vec<Alert>, AppError> {
        let mut query = self
            .conn
            .prepare(
                "SELECT id, product_id, chain, threshold_ore, threshold_kind, created,
                        last_triggered_interval
                 FROM alert ORDER BY id",
            )
            .map_err(|e| error(&self.path, e))?;
        let rows = query
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                    timestamp(row, 5)?,
                    row.get::<_, Option<i64>>(6)?,
                ))
            })
            .map_err(|e| error(&self.path, e))?;

        let mut alerts = Vec::new();
        for row in rows {
            let (id, product, chain, threshold, kind, created, last_triggered) =
                row.map_err(|e| error(&self.path, e))?;
            let chain = match chain.as_deref().map(Chain::from_slug) {
                None => None,
                Some(Some(chain)) => Some(chain),
                Some(None) => continue,
            };
            let Some(kind) = ThresholdKind::from_db_value(&kind) else {
                continue;
            };
            alerts.push(Alert {
                id,
                product: ProductId(product),
                chain,
                threshold: Ore(threshold),
                kind,
                created,
                last_triggered_interval: last_triggered,
            });
        }
        Ok(alerts)
    }

    /// Records that `alert` fired for the price in `interval`, so it stays quiet until the
    /// price changes.
    pub fn mark_alert_triggered(&mut self, alert: i64, interval: i64) -> Result<(), AppError> {
        self.conn
            .execute(
                "UPDATE alert SET last_triggered_interval = ?1 WHERE id = ?2",
                [interval, alert],
            )
            .map_err(|e| error(&self.path, e))?;
        Ok(())
    }
}
