//! Pant (SPEC §5.4). Pant er aldri med i pris eller literpris.

use serde::{Deserialize, Serialize};

use crate::modell::{Ml, Ore};

/// Pantesatser. Dette er data, ikke logikk – bekreft mot Infinitum ved behov.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PantKonfig {
    /// Pant for beholdere til og med `grense_ml`.
    pub liten_ore: Ore,
    /// Pant for beholdere over `grense_ml`.
    pub stor_ore: Ore,
    pub grense_ml: u32,
}

impl Default for PantKonfig {
    fn default() -> Self {
        Self {
            liten_ore: Ore(200),
            stor_ore: Ore(300),
            grense_ml: 500,
        }
    }
}

/// Pant for én beholder.
pub fn pant(volum: Ml, satser: &PantKonfig) -> Ore {
    if volum.get() <= satser.grense_ml {
        satser.liten_ore
    } else {
        satser.stor_ore
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grensen_er_inklusiv() {
        let satser = PantKonfig::default();
        let pant_for = |ml| pant(Ml::new(ml).unwrap(), &satser);
        assert_eq!(pant_for(250), Ore(200));
        assert_eq!(pant_for(500), Ore(200));
        assert_eq!(pant_for(501), Ore(300));
        assert_eq!(pant_for(1000), Ore(300));
    }
}
