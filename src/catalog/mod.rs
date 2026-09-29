//! The product catalog (SPEC §6): canonical products, EANs and matching.

pub mod file;
pub mod matching;
pub mod parse;

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::Path;

pub use file::Multipack;

use crate::error::AppError;
use crate::model::{Product, ProductId};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Catalog {
    pub products: Vec<Product>,
    pub multipacks: Vec<Multipack>,
}

impl Catalog {
    /// The catalog built into the binary.
    pub fn builtin() -> Result<Self, AppError> {
        Self::from_toml(file::BUILTIN)
            .map_err(|error| AppError::Catalog(format!("innebygd: {error}")))
    }

    /// The built-in catalog with the user's override (if the file exists) on top.
    pub fn load(override_path: &Path) -> Result<Self, AppError> {
        let mut catalog = Self::builtin()?;
        match fs::read_to_string(override_path) {
            Ok(text) => {
                let own = Self::from_toml(&text).map_err(|error| {
                    AppError::Catalog(format!("{}: {error}", override_path.display()))
                })?;
                catalog.merge(own);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        catalog.validate().map_err(AppError::Catalog)?;
        Ok(catalog)
    }

    pub fn from_toml(text: &str) -> Result<Self, String> {
        let file: file::CatalogFile = toml::from_str(text).map_err(|error| error.to_string())?;
        Ok(Self {
            products: file.products,
            multipacks: file.multipacks,
        })
    }

    /// Lays `other` over this one: the same product `id` or multipack `gtin` is replaced.
    pub fn merge(&mut self, other: Catalog) {
        for product in other.products {
            match self.products.iter_mut().find(|p| p.id == product.id) {
                Some(existing) => *existing = product,
                None => self.products.push(product),
            }
        }
        for pack in other.multipacks {
            match self.multipacks.iter_mut().find(|p| p.gtin == pack.gtin) {
                Some(existing) => *existing = pack,
                None => self.multipacks.push(pack),
            }
        }
    }

    /// Returns a Norwegian error message, since it is shown to the user.
    pub fn validate(&self) -> Result<(), String> {
        let mut ids = HashSet::new();
        for product in &self.products {
            if !ids.insert(&product.id) {
                return Err(format!("produkt-id «{}» finnes flere ganger", product.id));
            }
        }

        let mut gtins = HashSet::new();
        let mut check_gtin = |gtin: &str, owner: &ProductId| -> Result<(), String> {
            let normalized = matching::normalize_gtin(gtin)
                .ok_or_else(|| format!("ugyldig EAN «{gtin}» på «{owner}»"))?;
            if !gtins.insert(normalized) {
                return Err(format!("EAN «{gtin}» er brukt flere ganger"));
            }
            Ok(())
        };
        for product in &self.products {
            for gtin in &product.gtin {
                check_gtin(gtin, &product.id)?;
            }
        }
        for pack in &self.multipacks {
            if !ids.contains(&pack.product) {
                return Err(format!(
                    "flerpakning {} viser til ukjent produkt «{}»",
                    pack.gtin, pack.product
                ));
            }
            if pack.count < 2 {
                return Err(format!("flerpakning {} må ha antall ≥ 2", pack.gtin));
            }
            check_gtin(&pack.gtin, &pack.product)?;
        }
        Ok(())
    }

    pub fn find(&self, id: &ProductId) -> Option<&Product> {
        self.products.iter().find(|p| &p.id == id)
    }

    /// All known EANs, normalized – used by the sources for lookups.
    pub fn all_gtins(&self) -> Vec<String> {
        let singles = self.products.iter().flat_map(|p| p.gtin.iter());
        let packs = self.multipacks.iter().map(|p| &p.gtin);
        singles
            .chain(packs)
            .filter_map(|g| matching::normalize_gtin(g))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_catalog_is_valid() {
        let catalog = Catalog::builtin().unwrap();
        catalog.validate().unwrap();
        for brand in ["monster", "red-bull", "burn", "nocco", "battery"] {
            assert!(
                catalog.products.iter().any(|p| p.brand == brand),
                "missing {brand}"
            );
        }
        // The catalog must have real EANs, otherwise nothing matches.
        assert!(catalog.all_gtins().len() > 150);
    }

    #[test]
    fn override_replaces_and_adds() {
        let mut catalog = Catalog::builtin().unwrap();
        let product_count = catalog.products.len();
        let gtin_count = catalog.all_gtins().len();
        // The override replaces the whole product, including its EAN list.
        let replaced_gtins = catalog
            .find(&ProductId("monster-ultra-white-500-boks".into()))
            .map_or(0, |p| p.gtin.len());
        let own = Catalog::from_toml(
            r#"
            [[produkt]]
            id = "monster-ultra-white-500-boks"
            navn = "Monster Ultra White (egen)"
            merke = "monster"
            smak = "white"
            sukkerfri = true
            volum_ml = 500
            beholder = "boks"
            gtin = ["5060337500401"]

            [[produkt]]
            id = "xtra-energidrikk-500-boks"
            navn = "Xtra Energidrikk"
            merke = "xtra"
            smak = "original"
            sukkerfri = false
            volum_ml = 500
            beholder = "boks"
            egenmerke = true

            [[flerpakning]]
            gtin = "5060337500418"
            produkt = "monster-ultra-white-500-boks"
            antall = 4
            "#,
        )
        .unwrap();
        catalog.merge(own);
        catalog.validate().unwrap();
        assert_eq!(catalog.products.len(), product_count + 1);
        let id = ProductId("monster-ultra-white-500-boks".into());
        assert_eq!(
            catalog.find(&id).unwrap().name,
            "Monster Ultra White (egen)"
        );
        assert_eq!(catalog.all_gtins().len(), gtin_count - replaced_gtins + 2);
        let own_brand = ProductId("xtra-energidrikk-500-boks".into());
        assert!(catalog.find(&own_brand).unwrap().store_brand);
    }

    #[test]
    fn duplicate_ean_is_rejected() {
        let catalog = Catalog::from_toml(
            r#"
            [[produkt]]
            id = "a"
            navn = "A"
            merke = "x"
            smak = "a"
            sukkerfri = false
            volum_ml = 500
            beholder = "boks"
            gtin = ["7040110569908"]

            [[produkt]]
            id = "b"
            navn = "B"
            merke = "x"
            smak = "b"
            sukkerfri = false
            volum_ml = 500
            beholder = "boks"
            gtin = ["07040110569908"]
            "#,
        )
        .unwrap();
        assert!(catalog.validate().is_err());
    }

    #[test]
    fn unknown_field_is_rejected() {
        let result = Catalog::from_toml(
            r#"
            [[produkt]]
            id = "a"
            navn = "A"
            merke = "x"
            smak = "a"
            sukkerfrie = false
            volum_ml = 500
            beholder = "boks"
            "#,
        );
        assert!(result.is_err());
    }
}
