//! Deposit, "pant" (SPEC §5.4). The deposit is never part of the price or the liter price.

use serde::{Deserialize, Serialize};

use crate::model::{Ml, Ore};

/// Deposit rates, from `[pant]` in the config. This is data, not logic – check against
/// Infinitum when needed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DepositConfig {
    /// Deposit for containers up to and including `threshold_ml`.
    #[serde(rename = "liten_ore")]
    pub small: Ore,
    /// Deposit for containers above `threshold_ml`.
    #[serde(rename = "stor_ore")]
    pub large: Ore,
    #[serde(rename = "grense_ml")]
    pub threshold_ml: u32,
}

impl Default for DepositConfig {
    fn default() -> Self {
        Self {
            small: Ore(200),
            large: Ore(300),
            threshold_ml: 500,
        }
    }
}

/// Deposit for one container.
pub fn deposit(volume: Ml, rates: &DepositConfig) -> Ore {
    if volume.get() <= rates.threshold_ml {
        rates.small
    } else {
        rates.large
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threshold_is_inclusive() {
        let rates = DepositConfig::default();
        let deposit_for = |ml| deposit(Ml::new(ml).unwrap(), &rates);
        assert_eq!(deposit_for(250), Ore(200));
        assert_eq!(deposit_for(500), Ore(200));
        assert_eq!(deposit_for(501), Ore(300));
        assert_eq!(deposit_for(1000), Ore(300));
    }
}
