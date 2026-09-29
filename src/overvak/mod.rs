//! Prisvarsler (SPEC §7.7). Lagring, sjekk etter `oppdater` og skrivebordsvarsler
//! kommer i M4.

use jiff::Timestamp;
use serde::Serialize;

use crate::modell::{Kjede, Ore, ProduktId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Grensetype {
    /// Effektiv pris per beholder.
    Enhetspris,
    Literpris,
}

impl Grensetype {
    pub const fn slug(self) -> &'static str {
        match self {
            Grensetype::Enhetspris => "enhetspris",
            Grensetype::Literpris => "literpris",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Varsel {
    pub id: i64,
    pub produkt: ProduktId,
    pub kjede: Option<Kjede>,
    pub grense_ore: Ore,
    pub grensetype: Grensetype,
    pub opprettet: Timestamp,
}

impl Varsel {
    /// `--under 25` betyr strengt under 25 kr.
    pub fn utloses_av(&self, enhetspris: Ore, literpris: Ore) -> bool {
        let pris = match self.grensetype {
            Grensetype::Enhetspris => enhetspris,
            Grensetype::Literpris => literpris,
        };
        pris < self.grense_ore
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grensen_er_streng() {
        let varsel = Varsel {
            id: 1,
            produkt: ProduktId("p".into()),
            kjede: None,
            grense_ore: Ore(2500),
            grensetype: Grensetype::Enhetspris,
            opprettet: Timestamp::UNIX_EPOCH,
        };
        assert!(varsel.utloses_av(Ore(2490), Ore(4980)));
        assert!(!varsel.utloses_av(Ore(2500), Ore(5000)));
    }
}
