//! Matching av rå oppføringer mot katalogen (SPEC §6.3).
//!
//! Trinn 1 (EAN) er på plass. Trinn 2 (uskarp navnematching) kommer i M1; til da blir
//! oppføringer uten kjent EAN stående som umatchede.

use std::collections::HashMap;

use super::Katalog;
use crate::modell::{ProduktId, RaaOppforing};

/// Normaliserer en GTIN/EAN til 14 sifre, slik at EAN-13 og GTIN-14 med ledende null
/// blir like. `None` hvis strengen ikke er en plausibel GTIN.
pub fn normaliser_gtin(gtin: &str) -> Option<String> {
    let sifre: String = gtin.chars().filter(|c| !c.is_whitespace()).collect();
    if !(8..=14).contains(&sifre.len()) || !sifre.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(format!("{sifre:0>14}"))
}

/// Resultatet av en vellykket matching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Treff {
    pub produkt: ProduktId,
    /// Antall beholdere i den salgbare enheten.
    pub antall: u32,
    /// `true` for EAN-treff, `false` for uskarpe navnetreff (vises med `?`).
    pub verifisert: bool,
}

/// Oppslag fra normalisert EAN til produkt og pakningsstørrelse.
#[derive(Debug, Clone, Default)]
pub struct GtinIndeks(HashMap<String, (ProduktId, u32)>);

impl GtinIndeks {
    pub fn bygg(katalog: &Katalog) -> Self {
        let mut indeks = HashMap::new();
        for produkt in &katalog.produkter {
            for gtin in &produkt.gtin {
                if let Some(gtin) = normaliser_gtin(gtin) {
                    indeks.insert(gtin, (produkt.id.clone(), 1));
                }
            }
        }
        for pakke in &katalog.flerpakninger {
            if let Some(gtin) = normaliser_gtin(&pakke.gtin) {
                indeks.insert(gtin, (pakke.produkt.clone(), pakke.antall));
            }
        }
        Self(indeks)
    }

    pub fn finn(&self, gtin: &str) -> Option<(&ProduktId, u32)> {
        let gtin = normaliser_gtin(gtin)?;
        self.0.get(&gtin).map(|(id, antall)| (id, *antall))
    }
}

pub fn match_oppforing(indeks: &GtinIndeks, oppforing: &RaaOppforing) -> Option<Treff> {
    let gtin = oppforing.gtin.as_deref()?;
    let (produkt, antall) = indeks.finn(gtin)?;
    Some(Treff {
        produkt: produkt.clone(),
        antall,
        verifisert: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gtin_normaliseres_til_14_sifre() {
        assert_eq!(
            normaliser_gtin("7040110569908").as_deref(),
            Some("07040110569908")
        );
        assert_eq!(
            normaliser_gtin("07040110569908"),
            normaliser_gtin("7040110569908")
        );
        assert_eq!(normaliser_gtin("12345"), None);
        assert_eq!(normaliser_gtin("70401105699O8"), None);
    }

    #[test]
    fn indeks_finner_enkelt_og_flerpakning() {
        let katalog = Katalog::fra_toml(
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
        let indeks = GtinIndeks::bygg(&katalog);
        let p = ProduktId("p".into());
        assert_eq!(indeks.finn("07040110569908"), Some((&p, 1)));
        assert_eq!(indeks.finn("7040110569915"), Some((&p, 4)));
        assert_eq!(indeks.finn("7040110569922"), None);
    }
}
