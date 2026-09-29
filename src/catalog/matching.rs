//! Matching raw listings against the catalog (SPEC §6.3).
//!
//! Step 1 (EAN) is in place. Step 2 (fuzzy name matching) comes later in M1; until then,
//! listings without a known EAN stay unmatched.

use std::collections::HashMap;

use super::Catalog;
use crate::model::{ProductId, RawListing};

/// Normalizes a GTIN/EAN to 14 digits, so EAN-13 and GTIN-14 with a leading zero compare
/// equal. `None` if the string is not a plausible GTIN.
pub fn normalize_gtin(gtin: &str) -> Option<String> {
    let digits: String = gtin.chars().filter(|c| !c.is_whitespace()).collect();
    if !(8..=14).contains(&digits.len()) || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(format!("{digits:0>14}"))
}

/// A successful match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub product: ProductId,
    /// The pack size the catalog gives for the EAN. Only used as a cross-check – the
    /// listing's own name decides the pack size.
    pub pack_size: u32,
    /// `true` for EAN matches, `false` for fuzzy name matches (shown with `?`).
    pub verified: bool,
}

/// Lookup from normalized EAN to product and pack size.
#[derive(Debug, Clone, Default)]
pub struct GtinIndex(HashMap<String, (ProductId, u32)>);

impl GtinIndex {
    pub fn build(catalog: &Catalog) -> Self {
        let mut index = HashMap::new();
        for product in &catalog.products {
            for gtin in &product.gtin {
                if let Some(gtin) = normalize_gtin(gtin) {
                    index.insert(gtin, (product.id.clone(), 1));
                }
            }
        }
        for pack in &catalog.multipacks {
            if let Some(gtin) = normalize_gtin(&pack.gtin) {
                index.insert(gtin, (pack.product.clone(), pack.count));
            }
        }
        Self(index)
    }

    pub fn find(&self, gtin: &str) -> Option<(&ProductId, u32)> {
        let gtin = normalize_gtin(gtin)?;
        self.0.get(&gtin).map(|(id, count)| (id, *count))
    }
}

pub fn match_listing(index: &GtinIndex, listing: &RawListing) -> Option<Match> {
    let gtin = listing.gtin.as_deref()?;
    let (product, pack_size) = index.find(gtin)?;
    Some(Match {
        product: product.clone(),
        pack_size,
        verified: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gtin_is_normalized_to_14_digits() {
        assert_eq!(
            normalize_gtin("7040110569908").as_deref(),
            Some("07040110569908")
        );
        assert_eq!(
            normalize_gtin("07040110569908"),
            normalize_gtin("7040110569908")
        );
        assert_eq!(normalize_gtin("12345"), None);
        assert_eq!(normalize_gtin("70401105699O8"), None);
    }

    #[test]
    fn index_finds_singles_and_multipacks() {
        let catalog = Catalog::from_toml(
            r#"
            [[produkt]]
            id = "p"
            navn = "P"
            merke = "x"
            smak = "a"
            sukkerfri = false
            volum_ml = 500
            beholder = "boks"
            gtin = ["7040110569908"]

            [[flerpakning]]
            gtin = "7040110569915"
            produkt = "p"
            antall = 4
            "#,
        )
        .unwrap();
        let index = GtinIndex::build(&catalog);
        let p = ProductId("p".into());
        assert_eq!(index.find("07040110569908"), Some((&p, 1)));
        assert_eq!(index.find("7040110569915"), Some((&p, 4)));
        assert_eq!(index.find("7040110569922"), None);
    }
}
