//! Price alerts (SPEC §7.7). Storage, checking after `oppdater` and desktop
//! notifications come in M4.

use jiff::Timestamp;
use serde::Serialize;

use crate::model::{Chain, Ore, ProductId};

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
}

/// An alert as shown in JSON (Norwegian keys).
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
}

impl Alert {
    /// `--under 25` means strictly below 25 kr.
    pub fn triggered_by(&self, unit_price: Ore, liter_price: Ore) -> bool {
        let price = match self.kind {
            ThresholdKind::UnitPrice => unit_price,
            ThresholdKind::LiterPrice => liter_price,
        };
        price < self.threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threshold_is_strict() {
        let alert = Alert {
            id: 1,
            product: ProductId("p".into()),
            chain: None,
            threshold: Ore(2500),
            kind: ThresholdKind::UnitPrice,
            created: Timestamp::UNIX_EPOCH,
        };
        assert!(alert.triggered_by(Ore(2490), Ore(4980)));
        assert!(!alert.triggered_by(Ore(2500), Ore(5000)));
    }
}
