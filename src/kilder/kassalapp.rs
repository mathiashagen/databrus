//! Kassalapp-API-et – primærkilden for grunnpriser (SPEC §4.2, funn i §4.5).
//!
//! Henter hele energidrikkategorien side for side. Hver rad er ett produkt hos én butikk,
//! med hyllepris og tidspunktet prisen sist ble sett. Kassalapp har ingen tilbuds- eller
//! medlemsprisdata.

use std::collections::HashSet;
use std::num::NonZeroU32;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::Url;
use serde::Deserialize;

use super::{HenteKontekst, Kilde, KildeFeil};
use crate::katalog::tolk::tolk_pakke;
use crate::modell::{KildeId, Kjede, Ore, RaaOppforing};

pub const BASIS_URL: &str = "https://kassal.app/api/v1";
/// Overstyrer `BASIS_URL`. Brukes av ende-til-ende-testene mot en lokal etterligning.
pub const ENV_BASIS_URL: &str = "DATABRUS_KASSALAPP_URL";
/// Bekreftet via `X-RateLimit-Limit` 2026-09-29.
pub const FORESPORSLER_PER_MINUTT: NonZeroU32 = NonZeroU32::new(60).unwrap();
/// Kategorien «Energidrikk».
pub const KATEGORI_ENERGIDRIKK: u32 = 111;
/// Største sidestørrelse API-et tillater.
pub const SIDESTORRELSE: u32 = 100;
/// Sikring mot endeløs paginering. Kategorien er i dag 11 sider.
const MAKS_SIDER: u32 = 50;

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

pub struct Kassalapp {
    basis_url: String,
}

impl Default for Kassalapp {
    fn default() -> Self {
        Self::med_basis_url(BASIS_URL)
    }
}

impl Kassalapp {
    /// Standard-URL-en, eller `DATABRUS_KASSALAPP_URL` når den er satt.
    pub fn fra_miljo() -> Self {
        match std::env::var(ENV_BASIS_URL) {
            Ok(url) if !url.trim().is_empty() => Self::med_basis_url(url),
            _ => Self::default(),
        }
    }

    /// For tester mot en lokal etterligning av API-et.
    pub fn med_basis_url(basis_url: impl Into<String>) -> Self {
        Self {
            basis_url: basis_url.into().trim_end_matches('/').to_owned(),
        }
    }

    fn side_url(&self, side: u32) -> String {
        format!(
            "{}/products?category_id={KATEGORI_ENERGIDRIKK}&size={SIDESTORRELSE}&page={side}",
            self.basis_url
        )
    }
}

#[async_trait]
impl Kilde for Kassalapp {
    fn id(&self) -> KildeId {
        KildeId::Kassalapp
    }

    fn kjeder(&self) -> &'static [Kjede] {
        &KJEDER
    }

    async fn hent(&self, ctx: &HenteKontekst) -> Result<Vec<RaaOppforing>, KildeFeil> {
        let Some(nokkel) = ctx.api_nokkel.as_deref() else {
            return Err(KildeFeil::ManglerApiNokkel);
        };
        if let Some(vert) = Url::parse(&self.basis_url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned))
        {
            ctx.http.sett_takt(&vert, FORESPORSLER_PER_MINUTT);
        }

        let mut oppforinger = Vec::new();
        let mut sett = HashSet::new();
        let mut hoppet_over = 0usize;
        for side in 1..=MAKS_SIDER {
            let svar: Side = ctx
                .http
                .hent_json(&self.side_url(side), Some(nokkel))
                .await?;
            let tom = svar.data.is_empty();
            for produkt in svar.data {
                match tolk_produkt(produkt) {
                    Some(o) if sett.insert((o.kjede, o.kilde_produkt_id.clone())) => {
                        oppforinger.push(o);
                    }
                    Some(_) => {}
                    None => hoppet_over += 1,
                }
            }
            // `links.next` følges ikke: den mister `category_id`. Den sier bare om det
            // finnes flere sider.
            if tom || svar.links.next.is_none() {
                tracing::debug!(
                    "kassalapp: {} oppføringer fra {side} sider, {hoppet_over} hoppet over",
                    oppforinger.len()
                );
                return Ok(oppforinger);
            }
        }
        tracing::warn!("kassalapp: stoppet etter {MAKS_SIDER} sider");
        Ok(oppforinger)
    }
}

#[derive(Debug, Deserialize)]
struct Side {
    data: Vec<ApiProdukt>,
    #[serde(default)]
    links: Lenker,
}

#[derive(Debug, Default, Deserialize)]
struct Lenker {
    next: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiProdukt {
    id: u64,
    name: String,
    brand: Option<String>,
    ean: Option<String>,
    current_price: Option<ApiPris>,
    weight: Option<f64>,
    weight_unit: Option<String>,
    store: Option<ApiButikk>,
    updated_at: Option<String>,
}

/// Et tall i søk, et objekt med egen dato i EAN-oppslag.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ApiPris {
    Tall(f64),
    Objekt {
        price: Option<f64>,
        date: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
struct ApiButikk {
    code: Option<String>,
}

/// Gjør én API-rad om til en oppføring. `None` for rader uten pris eller uten kjent kjede.
fn tolk_produkt(produkt: ApiProdukt) -> Option<RaaOppforing> {
    let kjede = kjede_fra_kode(produkt.store?.code.as_deref()?)?;
    let (pris, pris_tidspunkt) = match produkt.current_price? {
        ApiPris::Tall(pris) => (pris, None),
        ApiPris::Objekt { price, date } => (price?, date),
    };
    let tidspunkt = pris_tidspunkt
        .or(produkt.updated_at)
        .and_then(|t| tolk_tidspunkt(&t));
    let raanavn = produkt.name.trim().to_owned();

    Some(RaaOppforing {
        kilde: KildeId::Kassalapp,
        kjede,
        kilde_produkt_id: produkt.id.to_string(),
        gtin: produkt.ean.filter(|e| !e.trim().is_empty()),
        antall: tolk_pakke(&raanavn).unwrap_or(1),
        raanavn,
        raamerke: produkt.brand.filter(|b| !b.trim().is_empty()),
        raa_storrelse: storrelse(produkt.weight, produkt.weight_unit.as_deref()),
        hyllepris: kroner_til_ore(pris)?,
        medlemspris: None,
        tilbud: None,
        tilgjengelig: None,
        pant: None,
        kilde_tidspunkt: tidspunkt,
    })
}

/// `32.9` → `Ore(3290)`. Null, negative og absurde beløp avvises.
fn kroner_til_ore(kroner: f64) -> Option<Ore> {
    if !kroner.is_finite() || kroner <= 0.0 || kroner > 1_000_000.0 {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)]
    Some(Ore((kroner * 100.0).round() as i64))
}

/// `weight` og `weight_unit` som tekst, f.eks. «500 ml» eller «0.5 l». Enheten mangler
/// ofte, og null-vekt betyr ukjent.
fn storrelse(vekt: Option<f64>, enhet: Option<&str>) -> Option<String> {
    let vekt = vekt.filter(|v| *v > 0.0)?;
    Some(match enhet.map(str::trim).filter(|e| !e.is_empty()) {
        Some(enhet) => format!("{vekt} {enhet}"),
        None => vekt.to_string(),
    })
}

/// Kassalapp bruker mikrosekunder («…07:00:28.000000Z»); vi lagrer hele sekunder.
fn tolk_tidspunkt(tekst: &str) -> Option<Timestamp> {
    let tidspunkt: Timestamp = tekst.parse().ok()?;
    Timestamp::from_second(tidspunkt.as_second()).ok()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    const KATEGORI: &str = include_str!("../../tests/fixtures/kassalapp/kategori-energidrikk.json");
    const SISTE_SIDE: &str =
        include_str!("../../tests/fixtures/kassalapp/kategori-energidrikk-siste-side.json");
    const EAN: &str = include_str!("../../tests/fixtures/kassalapp/ean-5060166693732.json");

    fn side(json: &str) -> Vec<RaaOppforing> {
        let side: Side = serde_json::from_str(json).unwrap();
        side.data.into_iter().filter_map(tolk_produkt).collect()
    }

    fn finn<'a>(rader: &'a [RaaOppforing], id: &str) -> &'a RaaOppforing {
        rader.iter().find(|r| r.kilde_produkt_id == id).unwrap()
    }

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

    #[test]
    fn kategoriside_tolkes() {
        let rader = side(KATEGORI);
        // 14 rader i fixturen, hvorav 2 fra COOP_NO.
        assert_eq!(rader.len(), 12);
        assert!(rader.iter().all(|r| r.kilde == KildeId::Kassalapp));

        let red_bull = finn(&rader, "1798");
        assert_eq!(red_bull.kjede, Kjede::Joker);
        assert_eq!(red_bull.hyllepris, Ore(4090));
        assert_eq!(red_bull.gtin.as_deref(), Some("9002490241407"));
        assert_eq!(red_bull.raa_storrelse.as_deref(), Some("473 ml"));
        assert_eq!(red_bull.antall, 1);
        assert_eq!(
            red_bull.kilde_tidspunkt,
            Some("2026-09-29T07:00:22Z".parse().unwrap())
        );
    }

    #[test]
    fn heltallspris_manglende_enhet_og_nullvekt() {
        let rader = side(KATEGORI);
        assert_eq!(finn(&rader, "4441").hyllepris, Ore(2000));
        assert_eq!(finn(&rader, "1955").raa_storrelse.as_deref(), Some("473"));
        assert_eq!(finn(&rader, "4667").raa_storrelse, None);
    }

    #[test]
    fn flerpakning_fra_navnet() {
        let rader = side(SISTE_SIDE);
        let pakke = finn(&rader, "225194");
        assert_eq!(pakke.antall, 4);
        assert_eq!(pakke.hyllepris, Ore(9960));
    }

    #[test]
    fn ean_oppslag_med_prisobjekt_og_egen_dato() {
        let svar: serde_json::Value = serde_json::from_str(EAN).unwrap();
        let rader: Vec<RaaOppforing> = svar["data"]["products"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| serde_json::from_value::<ApiProdukt>(p.clone()).unwrap())
            .filter_map(tolk_produkt)
            .collect();
        // 12 produkter: COOP_NO og de to uten butikk og pris hoppes over.
        assert_eq!(rader.len(), 9);
        let kiwi = rader.iter().find(|r| r.kjede == Kjede::Kiwi).unwrap();
        assert_eq!(kiwi.hyllepris, Ore(2390));
        // Prisens egen dato, ikke produktets `updated_at`.
        assert_eq!(
            kiwi.kilde_tidspunkt,
            Some("2023-04-14T07:00:41Z".parse().unwrap())
        );
    }

    #[test]
    fn kroneomregning() {
        assert_eq!(kroner_til_ore(16.9), Some(Ore(1690)));
        assert_eq!(kroner_til_ore(32.9), Some(Ore(3290)));
        assert_eq!(kroner_til_ore(735.31), Some(Ore(73_531)));
        assert_eq!(kroner_til_ore(20.0), Some(Ore(2000)));
        assert_eq!(kroner_til_ore(0.0), None);
        assert_eq!(kroner_til_ore(-5.0), None);
        assert_eq!(kroner_til_ore(f64::NAN), None);
    }

    #[test]
    fn side_url_har_kategori_og_storrelse() {
        let kilde = Kassalapp::med_basis_url("http://127.0.0.1:1234/api/v1/");
        assert_eq!(
            kilde.side_url(3),
            "http://127.0.0.1:1234/api/v1/products?category_id=111&size=100&page=3"
        );
    }
}
