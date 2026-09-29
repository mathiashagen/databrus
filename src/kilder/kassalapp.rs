//! Kassalapp-API-et – primærkilden for grunnpriser (SPEC §4.2, funn i §4.5).
//! Selve hentingen implementeres i M1.

use async_trait::async_trait;

use super::{HenteKontekst, Kilde, KildeFeil};
use crate::modell::{KildeId, Kjede, RaaOppforing};

pub const BASIS_URL: &str = "https://kassal.app/api/v1";
pub const VERT: &str = "kassal.app";
/// Bekreftet via `X-RateLimit-Limit` 2026-09-29.
pub const FORESPORSLER_PER_MINUTT: u32 = 60;
/// Kategorien «Energidrikk».
pub const KATEGORI_ENERGIDRIKK: u32 = 111;
/// Største sidestørrelse API-et tillater.
pub const SIDESTORRELSE: u32 = 100;

/// Kjedene Kassalapp har butikkoder for. Coop mangler: `COOP_NO` dekker alle Coop-kjedene
/// under ett og kan ikke fordeles på Extra/Obs/Mega/Prix (SPEC §4.5).
const KJEDER: [Kjede; 11] = [
    Kjede::Rema,
    Kjede::Kiwi,
    Kjede::Meny,
    Kjede::Spar,
    Kjede::Joker,
    Kjede::Bunnpris,
    Kjede::Oda,
    Kjede::Europris,
    Kjede::Engrossnett,
    Kjede::Havaristen,
    Kjede::Fastcandy,
];

/// Oversetter Kassalapps `store.code` til en kjede. `None` for ukjente koder og for
/// `COOP_NO`.
pub fn kjede_fra_kode(kode: &str) -> Option<Kjede> {
    Some(match kode {
        "REMA_1000" => Kjede::Rema,
        "KIWI" => Kjede::Kiwi,
        "MENY_NO" => Kjede::Meny,
        "SPAR_NO" => Kjede::Spar,
        "JOKER_NO" => Kjede::Joker,
        "BUNNPRIS" => Kjede::Bunnpris,
        "ODA_NO" => Kjede::Oda,
        "EUROPRIS_NO" => Kjede::Europris,
        "ENGROSSNETT_NO" => Kjede::Engrossnett,
        "HAVARISTEN" => Kjede::Havaristen,
        "FASTCANDY" => Kjede::Fastcandy,
        _ => return None,
    })
}

pub struct Kassalapp;

#[async_trait]
impl Kilde for Kassalapp {
    fn id(&self) -> KildeId {
        KildeId::Kassalapp
    }

    fn kjeder(&self) -> &'static [Kjede] {
        &KJEDER
    }

    async fn hent(&self, ctx: &HenteKontekst) -> Result<Vec<RaaOppforing>, KildeFeil> {
        if ctx.api_nokkel.is_none() {
            return Err(KildeFeil::ManglerApiNokkel);
        }
        Err(KildeFeil::IkkeImplementert)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    const KATEGORI: &str = include_str!("../../tests/fixtures/kassalapp/kategori-energidrikk.json");
    const EAN: &str = include_str!("../../tests/fixtures/kassalapp/ean-5060166693732.json");

    #[test]
    fn alle_kjeder_i_listen_har_en_kode() {
        let koder = [
            "REMA_1000",
            "KIWI",
            "MENY_NO",
            "SPAR_NO",
            "JOKER_NO",
            "BUNNPRIS",
            "ODA_NO",
            "EUROPRIS_NO",
            "ENGROSSNETT_NO",
            "HAVARISTEN",
            "FASTCANDY",
        ];
        let fra_koder: BTreeSet<_> = koder.iter().filter_map(|k| kjede_fra_kode(k)).collect();
        assert_eq!(fra_koder, KJEDER.into_iter().collect());
    }

    #[test]
    fn coop_og_ukjente_koder_ignoreres() {
        assert_eq!(kjede_fra_kode("COOP_NO"), None);
        assert_eq!(kjede_fra_kode("NY_KJEDE"), None);
    }

    /// Alle butikkoder i de ekte svarene er enten kjent eller bevisst ignorert.
    #[test]
    fn fixture_koder_er_kjent() {
        let mut koder = BTreeSet::new();
        let kategori: serde_json::Value = serde_json::from_str(KATEGORI).unwrap();
        let ean: serde_json::Value = serde_json::from_str(EAN).unwrap();
        let produkter = kategori["data"]
            .as_array()
            .unwrap()
            .iter()
            .chain(ean["data"]["products"].as_array().unwrap());
        for produkt in produkter {
            if let Some(kode) = produkt["store"]["code"].as_str() {
                koder.insert(kode.to_owned());
            }
        }
        assert!(koder.len() >= 5, "{koder:?}");
        for kode in koder {
            assert!(
                kode == "COOP_NO" || kjede_fra_kode(&kode).is_some(),
                "ukjent butikkode {kode}"
            );
        }
    }
}
