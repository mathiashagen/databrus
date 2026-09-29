//! The catalog file format (`data/katalog.toml` and the user's override). The keys are
//! Norwegian, since users edit the override file.

use serde::{Deserialize, Serialize};

use crate::model::{Product, ProductId};

/// The built-in catalog.
pub const BUILTIN: &str = include_str!("../../data/katalog.toml");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CatalogFile {
    #[serde(rename = "produkt", default)]
    pub products: Vec<Product>,
    #[serde(rename = "flerpakning", default)]
    pub multipacks: Vec<Multipack>,
}

/// A multipack EAN that points to a single product (SPEC §6.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Multipack {
    pub gtin: String,
    #[serde(rename = "produkt")]
    pub product: ProductId,
    #[serde(rename = "antall")]
    pub count: u32,
}
