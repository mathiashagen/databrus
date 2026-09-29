//! The Kassalapp API – the primary source of base prices (SPEC §4.2, findings in §4.5).
//!
//! Fetches the whole energy drink category page by page. Each row is one product at one
//! store, with the shelf price and when the price was last seen. Kassalapp has no offer
//! or member-price data.

use std::collections::HashSet;
use std::num::NonZeroU32;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::Url;
use serde::Deserialize;

use super::{FetchContext, Source, SourceError};
use crate::catalog::parse::parse_pack_size;
use crate::model::{Chain, Ore, RawListing, SourceId};

pub const BASE_URL: &str = "https://kassal.app/api/v1";
/// Overrides `BASE_URL`. Used by the end-to-end tests against a local mock.
pub const ENV_BASE_URL: &str = "DATABRUS_KASSALAPP_URL";
/// Confirmed via `X-RateLimit-Limit` on 2026-09-29.
pub const REQUESTS_PER_MINUTE: NonZeroU32 = NonZeroU32::new(60).unwrap();
/// The "Energidrikk" category.
pub const CATEGORY_ENERGY_DRINKS: u32 = 111;
/// The largest page size the API allows.
pub const PAGE_SIZE: u32 = 100;
/// Guard against endless pagination. The category is 11 pages today.
const MAX_PAGES: u32 = 50;

/// The chains Kassalapp has store codes for. Coop is missing: `COOP_NO` covers all Coop
/// chains at once and cannot be split into Extra/Obs/Mega/Prix (SPEC §4.5).
const CHAINS: [Chain; 11] = [
    Chain::Rema,
    Chain::Kiwi,
    Chain::Meny,
    Chain::Spar,
    Chain::Joker,
    Chain::Bunnpris,
    Chain::Oda,
    Chain::Europris,
    Chain::Engrossnett,
    Chain::Havaristen,
    Chain::Fastcandy,
];

/// Maps Kassalapp's `store.code` to a chain. `None` for unknown codes and for `COOP_NO`.
pub fn chain_from_code(code: &str) -> Option<Chain> {
    Some(match code {
        "REMA_1000" => Chain::Rema,
        "KIWI" => Chain::Kiwi,
        "MENY_NO" => Chain::Meny,
        "SPAR_NO" => Chain::Spar,
        "JOKER_NO" => Chain::Joker,
        "BUNNPRIS" => Chain::Bunnpris,
        "ODA_NO" => Chain::Oda,
        "EUROPRIS_NO" => Chain::Europris,
        "ENGROSSNETT_NO" => Chain::Engrossnett,
        "HAVARISTEN" => Chain::Havaristen,
        "FASTCANDY" => Chain::Fastcandy,
        _ => return None,
    })
}

pub struct Kassalapp {
    base_url: String,
}

impl Default for Kassalapp {
    fn default() -> Self {
        Self::with_base_url(BASE_URL)
    }
}

impl Kassalapp {
    /// The default URL, or `DATABRUS_KASSALAPP_URL` when set.
    pub fn from_env() -> Self {
        match std::env::var(ENV_BASE_URL) {
            Ok(url) if !url.trim().is_empty() => Self::with_base_url(url),
            _ => Self::default(),
        }
    }

    /// For tests against a local mock of the API.
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_owned(),
        }
    }

    fn page_url(&self, page: u32) -> String {
        format!(
            "{}/products?category_id={CATEGORY_ENERGY_DRINKS}&size={PAGE_SIZE}&page={page}",
            self.base_url
        )
    }
}

#[async_trait]
impl Source for Kassalapp {
    fn id(&self) -> SourceId {
        SourceId::Kassalapp
    }

    fn chains(&self) -> &'static [Chain] {
        &CHAINS
    }

    async fn fetch(&self, ctx: &FetchContext) -> Result<Vec<RawListing>, SourceError> {
        let Some(key) = ctx.api_key.as_deref() else {
            return Err(SourceError::MissingApiKey);
        };
        if let Some(host) = Url::parse(&self.base_url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned))
        {
            ctx.http.set_rate(&host, REQUESTS_PER_MINUTE);
        }

        let mut listings = Vec::new();
        let mut seen = HashSet::new();
        let mut skipped = 0usize;
        for page in 1..=MAX_PAGES {
            let response: Page = ctx.http.get_json(&self.page_url(page), Some(key)).await?;
            let empty = response.data.is_empty();
            for product in response.data {
                match parse_product(product) {
                    Some(l) if seen.insert((l.chain, l.source_product_id.clone())) => {
                        listings.push(l);
                    }
                    Some(_) => {}
                    None => skipped += 1,
                }
            }
            // `links.next` is not followed: it drops `category_id`. It only says whether
            // there are more pages.
            if empty || response.links.next.is_none() {
                tracing::debug!(
                    "kassalapp: {} oppføringer fra {page} sider, {skipped} hoppet over",
                    listings.len()
                );
                return Ok(listings);
            }
        }
        tracing::warn!("kassalapp: stoppet etter {MAX_PAGES} sider");
        Ok(listings)
    }
}

#[derive(Debug, Deserialize)]
struct Page {
    data: Vec<ApiProduct>,
    #[serde(default)]
    links: Links,
}

#[derive(Debug, Default, Deserialize)]
struct Links {
    next: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiProduct {
    id: u64,
    name: String,
    brand: Option<String>,
    ean: Option<String>,
    current_price: Option<ApiPrice>,
    weight: Option<f64>,
    weight_unit: Option<String>,
    store: Option<ApiStore>,
    updated_at: Option<String>,
}

/// A number in searches, an object with its own date in EAN lookups.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ApiPrice {
    Number(f64),
    Object {
        price: Option<f64>,
        date: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
struct ApiStore {
    code: Option<String>,
}

/// Turns one API row into a listing. `None` for rows without a price or a known chain.
fn parse_product(product: ApiProduct) -> Option<RawListing> {
    let chain = chain_from_code(product.store?.code.as_deref()?)?;
    let (price, price_time) = match product.current_price? {
        ApiPrice::Number(price) => (price, None),
        ApiPrice::Object { price, date } => (price?, date),
    };
    let timestamp = price_time
        .or(product.updated_at)
        .and_then(|t| parse_timestamp(&t));
    let raw_name = product.name.trim().to_owned();

    Some(RawListing {
        source: SourceId::Kassalapp,
        chain,
        source_product_id: product.id.to_string(),
        gtin: product.ean.filter(|e| !e.trim().is_empty()),
        pack_size: parse_pack_size(&raw_name).unwrap_or(1),
        raw_name,
        raw_brand: product.brand.filter(|b| !b.trim().is_empty()),
        raw_size: size_text(product.weight, product.weight_unit.as_deref()),
        shelf_price: kroner_to_ore(price)?,
        member_price: None,
        offer: None,
        available: None,
        deposit: None,
        source_timestamp: timestamp,
    })
}

/// `32.9` → `Ore(3290)`. Zero, negative and absurd amounts are rejected.
fn kroner_to_ore(kroner: f64) -> Option<Ore> {
    if !kroner.is_finite() || kroner <= 0.0 || kroner > 1_000_000.0 {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)]
    Some(Ore((kroner * 100.0).round() as i64))
}

/// `weight` and `weight_unit` as text, e.g. "500 ml" or "0.5 l". The unit is often
/// missing, and a zero weight means unknown.
fn size_text(weight: Option<f64>, unit: Option<&str>) -> Option<String> {
    let weight = weight.filter(|w| *w > 0.0)?;
    Some(match unit.map(str::trim).filter(|u| !u.is_empty()) {
        Some(unit) => format!("{weight} {unit}"),
        None => weight.to_string(),
    })
}

/// Kassalapp uses microseconds ("…07:00:28.000000Z"); we store whole seconds.
fn parse_timestamp(text: &str) -> Option<Timestamp> {
    let timestamp: Timestamp = text.parse().ok()?;
    Timestamp::from_second(timestamp.as_second()).ok()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    const CATEGORY: &str =
        include_str!("../../tests/fixtures/kassalapp/category-energy-drinks.json");
    const LAST_PAGE: &str =
        include_str!("../../tests/fixtures/kassalapp/category-energy-drinks-last-page.json");
    const EAN: &str = include_str!("../../tests/fixtures/kassalapp/ean-5060166693732.json");

    fn page(json: &str) -> Vec<RawListing> {
        let page: Page = serde_json::from_str(json).unwrap();
        page.data.into_iter().filter_map(parse_product).collect()
    }

    fn find<'a>(listings: &'a [RawListing], id: &str) -> &'a RawListing {
        listings.iter().find(|l| l.source_product_id == id).unwrap()
    }

    #[test]
    fn every_chain_in_the_list_has_a_code() {
        let codes = [
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
        let from_codes: BTreeSet<_> = codes.iter().filter_map(|c| chain_from_code(c)).collect();
        assert_eq!(from_codes, CHAINS.into_iter().collect());
    }

    #[test]
    fn coop_and_unknown_codes_are_ignored() {
        assert_eq!(chain_from_code("COOP_NO"), None);
        assert_eq!(chain_from_code("NY_KJEDE"), None);
    }

    /// Every store code in the real responses is either known or deliberately ignored.
    #[test]
    fn fixture_codes_are_known() {
        let mut codes = BTreeSet::new();
        let category: serde_json::Value = serde_json::from_str(CATEGORY).unwrap();
        let ean: serde_json::Value = serde_json::from_str(EAN).unwrap();
        let products = category["data"]
            .as_array()
            .unwrap()
            .iter()
            .chain(ean["data"]["products"].as_array().unwrap());
        for product in products {
            if let Some(code) = product["store"]["code"].as_str() {
                codes.insert(code.to_owned());
            }
        }
        assert!(codes.len() >= 5, "{codes:?}");
        for code in codes {
            assert!(
                code == "COOP_NO" || chain_from_code(&code).is_some(),
                "unknown store code {code}"
            );
        }
    }

    #[test]
    fn category_page_is_parsed() {
        let listings = page(CATEGORY);
        // 14 rows in the fixture, 2 of them from COOP_NO.
        assert_eq!(listings.len(), 12);
        assert!(listings.iter().all(|l| l.source == SourceId::Kassalapp));

        let red_bull = find(&listings, "1798");
        assert_eq!(red_bull.chain, Chain::Joker);
        assert_eq!(red_bull.shelf_price, Ore(4090));
        assert_eq!(red_bull.gtin.as_deref(), Some("9002490241407"));
        assert_eq!(red_bull.raw_size.as_deref(), Some("473 ml"));
        assert_eq!(red_bull.pack_size, 1);
        assert_eq!(
            red_bull.source_timestamp,
            Some("2026-09-29T07:00:22Z".parse().unwrap())
        );
    }

    #[test]
    fn integer_price_missing_unit_and_zero_weight() {
        let listings = page(CATEGORY);
        assert_eq!(find(&listings, "4441").shelf_price, Ore(2000));
        assert_eq!(find(&listings, "1955").raw_size.as_deref(), Some("473"));
        assert_eq!(find(&listings, "4667").raw_size, None);
    }

    #[test]
    fn pack_size_from_the_name() {
        let listings = page(LAST_PAGE);
        let pack = find(&listings, "225194");
        assert_eq!(pack.pack_size, 4);
        assert_eq!(pack.shelf_price, Ore(9960));
    }

    #[test]
    fn ean_lookup_with_price_object_and_its_own_date() {
        let response: serde_json::Value = serde_json::from_str(EAN).unwrap();
        let listings: Vec<RawListing> = response["data"]["products"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| serde_json::from_value::<ApiProduct>(p.clone()).unwrap())
            .filter_map(parse_product)
            .collect();
        // 12 products: COOP_NO and the two without store and price are skipped.
        assert_eq!(listings.len(), 9);
        let kiwi = listings.iter().find(|l| l.chain == Chain::Kiwi).unwrap();
        assert_eq!(kiwi.shelf_price, Ore(2390));
        // The price's own date, not the product's `updated_at`.
        assert_eq!(
            kiwi.source_timestamp,
            Some("2023-04-14T07:00:41Z".parse().unwrap())
        );
    }

    #[test]
    fn kroner_conversion() {
        assert_eq!(kroner_to_ore(16.9), Some(Ore(1690)));
        assert_eq!(kroner_to_ore(32.9), Some(Ore(3290)));
        assert_eq!(kroner_to_ore(735.31), Some(Ore(73_531)));
        assert_eq!(kroner_to_ore(20.0), Some(Ore(2000)));
        assert_eq!(kroner_to_ore(0.0), None);
        assert_eq!(kroner_to_ore(-5.0), None);
        assert_eq!(kroner_to_ore(f64::NAN), None);
    }

    #[test]
    fn page_url_has_category_and_size() {
        let source = Kassalapp::with_base_url("http://127.0.0.1:1234/api/v1/");
        assert_eq!(
            source.page_url(3),
            "http://127.0.0.1:1234/api/v1/products?category_id=111&size=100&page=3"
        );
    }
}
