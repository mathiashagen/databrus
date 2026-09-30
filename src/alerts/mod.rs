//! Price alerts (SPEC §7.7): which alerts fire for the current prices, and what they say.
//! Showing them (stderr, desktop) is up to the caller.

use jiff::Timestamp;
use serde::Serialize;

use crate::model::{Chain, Ore, ProductId, SearchResult};
use crate::output::format;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ThresholdKind {
    /// Effective price per container.
    #[serde(rename = "enhetspris")]
    UnitPrice,
    #[serde(rename = "literpris")]
    LiterPrice,
}

impl ThresholdKind {
    /// The value stored in the `alert.threshold_kind` column.
    pub const fn db_value(self) -> &'static str {
        match self {
            ThresholdKind::UnitPrice => "unit_price",
            ThresholdKind::LiterPrice => "liter_price",
        }
    }

    pub fn from_db_value(value: &str) -> Option<Self> {
        match value {
            "unit_price" => Some(ThresholdKind::UnitPrice),
            "liter_price" => Some(ThresholdKind::LiterPrice),
            _ => None,
        }
    }

    /// The price of a row that this kind of threshold looks at.
    pub fn price(self, row: &SearchResult) -> Ore {
        match self {
            ThresholdKind::UnitPrice => row.effective_unit_price,
            ThresholdKind::LiterPrice => row.liter_price,
        }
    }

    /// `19,90 kr` or `39,80 kr/l`.
    pub fn amount(self, price: Ore) -> String {
        match self {
            ThresholdKind::UnitPrice => format!("{} kr", format::kr(price)),
            ThresholdKind::LiterPrice => format!("{} kr/l", format::kr(price)),
        }
    }
}

/// An alert the user is adding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAlert {
    pub product: ProductId,
    pub chain: Option<Chain>,
    pub threshold: Ore,
    pub kind: ThresholdKind,
}

/// A stored alert, as shown in JSON (Norwegian keys).
#[derive(Debug, Clone, Serialize)]
pub struct Alert {
    pub id: i64,
    #[serde(rename = "produkt")]
    pub product: ProductId,
    #[serde(rename = "kjede")]
    pub chain: Option<Chain>,
    #[serde(rename = "grense_ore")]
    pub threshold: Ore,
    #[serde(rename = "grensetype")]
    pub kind: ThresholdKind,
    #[serde(rename = "opprettet")]
    pub created: Timestamp,
    /// The price interval the alert last fired for.
    #[serde(skip)]
    pub last_triggered_interval: Option<i64>,
}

impl Alert {
    /// `--under 25` means strictly below 25 kr.
    pub fn triggered_by(&self, row: &SearchResult) -> bool {
        self.kind.price(row) < self.threshold
    }

    /// The lowest current price for this alert among `rows` (the ranked current prices):
    /// its product, at its chain if it has one.
    pub fn best<'a>(&self, rows: &'a [SearchResult]) -> Option<&'a SearchResult> {
        rows.iter()
            .filter(|r| r.product.id == self.product && self.chain.is_none_or(|c| c == r.chain))
            .min_by_key(|r| (self.kind.price(r), r.liter_price))
    }
}

/// Alerts that should fire now: the best price is under the threshold, and the alert has
/// not already fired for that price interval (so an unchanged price does not notify
/// every day).
pub fn to_fire<'a>(
    alerts: &'a [Alert],
    rows: &'a [SearchResult],
) -> Vec<(&'a Alert, &'a SearchResult)> {
    alerts
        .iter()
        .filter_map(|alert| {
            let best = alert.best(rows)?;
            let new_price = alert.last_triggered_interval != Some(best.interval_id);
            (alert.triggered_by(best) && new_price).then_some((alert, best))
        })
        .collect()
}

/// The notification: a title and a line, e.g. «Prisvarsel: Monster Ultra White 0,5 l» and
/// «19,90 kr hos Kiwi – under grensen på 20,00 kr».
pub fn message(alert: &Alert, row: &SearchResult) -> (String, String) {
    let title = format!(
        "Prisvarsel: {} {}",
        row.product.name,
        format::liters(row.product.volume)
    );
    let body = format!(
        "{} hos {} – under grensen på {}",
        alert.kind.amount(alert.kind.price(row)),
        row.chain.display_name(),
        alert.kind.amount(alert.threshold)
    );
    (title, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::example_hits;

    fn alert(threshold: i64, kind: ThresholdKind, chain: Option<Chain>) -> Alert {
        Alert {
            id: 1,
            product: ProductId("monster-ultra-white-500-boks".into()),
            chain,
            threshold: Ore(threshold),
            kind,
            created: Timestamp::UNIX_EPOCH,
            last_triggered_interval: None,
        }
    }

    /// The example rows have Monster Ultra White at Kiwi (15,90, 31,80 kr/l), Coop Extra
    /// (21,90) and Oda (22,40).
    fn rows() -> Vec<SearchResult> {
        let mut rows = example_hits().rows;
        for (i, row) in rows.iter_mut().enumerate() {
            row.interval_id = i64::try_from(i).unwrap() + 10;
        }
        rows
    }

    #[test]
    fn threshold_is_strict() {
        let rows = rows();
        let kiwi = &rows[0];
        assert!(alert(1591, ThresholdKind::UnitPrice, None).triggered_by(kiwi));
        assert!(!alert(1590, ThresholdKind::UnitPrice, None).triggered_by(kiwi));
        assert!(alert(3181, ThresholdKind::LiterPrice, None).triggered_by(kiwi));
    }

    #[test]
    fn best_price_respects_the_chain() {
        let rows = rows();
        let any = alert(2000, ThresholdKind::UnitPrice, None);
        assert_eq!(any.best(&rows).map(|r| r.chain), Some(Chain::Kiwi));
        let oda = alert(2000, ThresholdKind::UnitPrice, Some(Chain::Oda));
        assert_eq!(oda.best(&rows).map(|r| r.chain), Some(Chain::Oda));
        let rema = alert(2000, ThresholdKind::UnitPrice, Some(Chain::Rema));
        assert!(rema.best(&rows).is_none());
    }

    #[test]
    fn fires_once_per_price_interval() {
        let rows = rows();
        let mut alerts = vec![alert(2000, ThresholdKind::UnitPrice, None)];
        assert_eq!(to_fire(&alerts, &rows).len(), 1);

        // Fired for Kiwi's interval: quiet until that price changes.
        alerts[0].last_triggered_interval = Some(10);
        assert!(to_fire(&alerts, &rows).is_empty());
        alerts[0].last_triggered_interval = Some(9);
        assert_eq!(to_fire(&alerts, &rows).len(), 1);
    }

    #[test]
    fn above_the_threshold_does_not_fire() {
        let alerts = [alert(1500, ThresholdKind::UnitPrice, None)];
        assert!(to_fire(&alerts, &rows()).is_empty());
    }

    #[test]
    fn message_is_norwegian() {
        let rows = rows();
        let (title, body) = message(&alert(2000, ThresholdKind::UnitPrice, None), &rows[0]);
        assert_eq!(title, "Prisvarsel: Monster Ultra White 0,5 l");
        assert_eq!(body, "15,90 kr hos Kiwi – under grensen på 20,00 kr");
        let (_, body) = message(&alert(4000, ThresholdKind::LiterPrice, None), &rows[0]);
        assert_eq!(body, "31,80 kr/l hos Kiwi – under grensen på 40,00 kr/l");
    }
}
