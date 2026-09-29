//! Filformatet for katalogen (`data/katalog.toml` og brukerens overstyring).

use serde::{Deserialize, Serialize};

use crate::modell::{Produkt, ProduktId};

/// Den innebygde katalogen.
pub const INNEBYGD: &str = include_str!("../../data/katalog.toml");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Katalogfil {
    #[serde(default)]
    pub produkt: Vec<Produkt>,
    #[serde(default)]
    pub flerpakning: Vec<Flerpakning>,
}

/// En flerpaknings-EAN som peker på et enkeltprodukt (SPEC §6.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Flerpakning {
    pub gtin: String,
    pub produkt: ProduktId,
    pub antall: u32,
}
