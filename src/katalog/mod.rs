//! Produktkatalogen (SPEC §6): kanoniske produkter, EAN-er og matching.

pub mod fil;
pub mod matching;
pub mod tolk;

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::Path;

pub use fil::Flerpakning;

use crate::feil::AppFeil;
use crate::modell::{Produkt, ProduktId};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Katalog {
    pub produkter: Vec<Produkt>,
    pub flerpakninger: Vec<Flerpakning>,
}

impl Katalog {
    /// Katalogen som er bygd inn i programmet.
    pub fn innebygd() -> Result<Self, AppFeil> {
        Self::fra_toml(fil::INNEBYGD).map_err(|feil| AppFeil::Katalog(format!("innebygd: {feil}")))
    }

    /// Den innebygde katalogen med brukerens overstyring (hvis filen finnes) lagt over.
    pub fn last(overstyring: &Path) -> Result<Self, AppFeil> {
        let mut katalog = Self::innebygd()?;
        match fs::read_to_string(overstyring) {
            Ok(tekst) => {
                let egen = Self::fra_toml(&tekst).map_err(|feil| {
                    AppFeil::Katalog(format!("{}: {feil}", overstyring.display()))
                })?;
                katalog.sla_sammen(egen);
            }
            Err(feil) if feil.kind() == io::ErrorKind::NotFound => {}
            Err(feil) => return Err(feil.into()),
        }
        katalog.valider().map_err(AppFeil::Katalog)?;
        Ok(katalog)
    }

    pub fn fra_toml(tekst: &str) -> Result<Self, String> {
        let fil: fil::Katalogfil = toml::from_str(tekst).map_err(|feil| feil.to_string())?;
        Ok(Self {
            produkter: fil.produkt,
            flerpakninger: fil.flerpakning,
        })
    }

    /// Legger `annen` over denne: samme produkt-`id` eller flerpaknings-`gtin` erstattes.
    pub fn sla_sammen(&mut self, annen: Katalog) {
        for produkt in annen.produkter {
            match self.produkter.iter_mut().find(|p| p.id == produkt.id) {
                Some(eksisterende) => *eksisterende = produkt,
                None => self.produkter.push(produkt),
            }
        }
        for pakke in annen.flerpakninger {
            match self.flerpakninger.iter_mut().find(|p| p.gtin == pakke.gtin) {
                Some(eksisterende) => *eksisterende = pakke,
                None => self.flerpakninger.push(pakke),
            }
        }
    }

    pub fn valider(&self) -> Result<(), String> {
        let mut ider = HashSet::new();
        for produkt in &self.produkter {
            if !ider.insert(&produkt.id) {
                return Err(format!("produkt-id «{}» finnes flere ganger", produkt.id));
            }
        }

        let mut gtiner = HashSet::new();
        let mut sjekk_gtin = |gtin: &str, eier: &ProduktId| -> Result<(), String> {
            let normalisert = matching::normaliser_gtin(gtin)
                .ok_or_else(|| format!("ugyldig EAN «{gtin}» på «{eier}»"))?;
            if !gtiner.insert(normalisert) {
                return Err(format!("EAN «{gtin}» er brukt flere ganger"));
            }
            Ok(())
        };
        for produkt in &self.produkter {
            for gtin in &produkt.gtin {
                sjekk_gtin(gtin, &produkt.id)?;
            }
        }
        for pakke in &self.flerpakninger {
            if !ider.contains(&pakke.produkt) {
                return Err(format!(
                    "flerpakning {} viser til ukjent produkt «{}»",
                    pakke.gtin, pakke.produkt
                ));
            }
            if pakke.antall < 2 {
                return Err(format!("flerpakning {} må ha antall ≥ 2", pakke.gtin));
            }
            sjekk_gtin(&pakke.gtin, &pakke.produkt)?;
        }
        Ok(())
    }

    pub fn finn(&self, id: &ProduktId) -> Option<&Produkt> {
        self.produkter.iter().find(|p| &p.id == id)
    }

    /// Alle kjente EAN-er, normalisert – brukes av kildene til oppslag.
    pub fn alle_gtin(&self) -> Vec<String> {
        let enkelt = self.produkter.iter().flat_map(|p| p.gtin.iter());
        let pakker = self.flerpakninger.iter().map(|p| &p.gtin);
        enkelt
            .chain(pakker)
            .filter_map(|g| matching::normaliser_gtin(g))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn innebygd_katalog_er_gyldig() {
        let katalog = Katalog::innebygd().unwrap();
        katalog.valider().unwrap();
        for merke in ["monster", "red-bull", "burn", "nocco", "battery"] {
            assert!(
                katalog.produkter.iter().any(|p| p.merke == merke),
                "mangler {merke}"
            );
        }
        // Katalogen skal ha ekte EAN-er, ellers matcher ingenting.
        assert!(katalog.alle_gtin().len() > 150);
    }

    #[test]
    fn overstyring_erstatter_og_legger_til() {
        let mut katalog = Katalog::innebygd().unwrap();
        let antall = katalog.produkter.len();
        let antall_gtin = katalog.alle_gtin().len();
        let egen = Katalog::fra_toml(
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
        katalog.sla_sammen(egen);
        katalog.valider().unwrap();
        assert_eq!(katalog.produkter.len(), antall + 1);
        let id = ProduktId("monster-ultra-white-500-boks".into());
        assert_eq!(
            katalog.finn(&id).unwrap().navn,
            "Monster Ultra White (egen)"
        );
        assert_eq!(katalog.alle_gtin().len(), antall_gtin + 2);
    }

    #[test]
    fn duplisert_ean_avvises() {
        let katalog = Katalog::fra_toml(
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
        assert!(katalog.valider().is_err());
    }

    #[test]
    fn ukjent_felt_avvises() {
        let feil = Katalog::fra_toml(
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
        assert!(feil.is_err());
    }
}
