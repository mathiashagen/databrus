//! Oda – the public JSON search API (SPEC §4.6).
//!
//! One search for "energidrikk" covers every energy drink Oda sells (brand searches only add
//! noise), in about two pages. Each product has the current price, availability and, when
//! discounted, the campaign and the undiscounted price. Oda reports no EAN; the catalog's
//! source links and the name matcher connect the listings to products.

use std::collections::HashSet;
use std::num::NonZeroU32;

use async_trait::async_trait;
use jiff::Timestamp;
use jiff::civil::Date;
use reqwest::Url;
use serde::Deserialize;

use super::{FetchContext, Source, SourceError};
use crate::catalog::parse::parse_pack_size;
use crate::model::{Chain, Offer, OfferInfo, Ore, RawListing, SourceId};
use crate::output::format::parse_kr;

pub const BASE_URL: &str = "https://oda.com/api/v1";
/// Overrides `BASE_URL`. Used by the end-to-end tests against a local mock.
pub const ENV_BASE_URL: &str = "DATABRUS_ODA_URL";
/// Oda documents no limit; stay far below anything noticeable.
pub const REQUESTS_PER_MINUTE: NonZeroU32 = NonZeroU32::new(30).unwrap();
const QUERY: &str = "energidrikk";
/// The largest page size Oda returns.
const PAGE_SIZE: u32 = 50;
/// Guard against endless pagination. The search is two pages today.
const MAX_PAGES: u32 = 10;

pub struct Oda {
    base_url: String,
}

impl Default for Oda {
    fn default() -> Self {
        Self::with_base_url(BASE_URL)
    }
}

impl Oda {
    /// The default URL, or `DATABRUS_ODA_URL` when set.
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
            "{}/search/mixed/?q={QUERY}&type=product&page={page}&size={PAGE_SIZE}",
            self.base_url
        )
    }
}

#[async_trait]
impl Source for Oda {
    fn id(&self) -> SourceId {
        SourceId::Oda
    }

    fn chains(&self) -> &'static [Chain] {
        &[Chain::Oda]
    }

    async fn fetch(&self, ctx: &FetchContext) -> Result<Vec<RawListing>, SourceError> {
        if let Some(host) = Url::parse(&self.base_url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned))
        {
            ctx.http.set_rate(&host, REQUESTS_PER_MINUTE);
        }

        let mut listings = Vec::new();
        let mut seen = HashSet::new();
        for page in 1..=MAX_PAGES {
            let response: Page = ctx.http.get_json(&self.page_url(page), None).await?;
            let empty = response.items.is_empty();
            for item in response.items {
                if item.kind != "product" {
                    continue;
                }
                // Other item shapes may appear in a mixed search; skip what does not parse.
                let Ok(product) = serde_json::from_value::<ApiProduct>(item.attributes) else {
                    continue;
                };
                if let Some(listing) = parse_product(product)
                    && seen.insert(listing.source_product_id.clone())
                {
                    listings.push(listing);
                }
            }
            if empty || !response.attributes.has_more_items {
                tracing::debug!("oda: {} oppføringer fra {page} sider", listings.len());
                return Ok(listings);
            }
        }
        tracing::warn!("oda: stoppet etter {MAX_PAGES} sider");
        Ok(listings)
    }
}

#[derive(Debug, Deserialize)]
struct Page {
    items: Vec<Item>,
    attributes: PageAttributes,
}

#[derive(Debug, Deserialize)]
struct PageAttributes {
    #[serde(default)]
    has_more_items: bool,
}

#[derive(Debug, Deserialize)]
struct Item {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    attributes: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct ApiProduct {
    id: u64,
    full_name: String,
    brand: Option<String>,
    name_extra: Option<String>,
    gross_price: String,
    discount: Option<ApiDiscount>,
    promotion: Option<ApiPromotion>,
    availability: Option<ApiAvailability>,
}

#[derive(Debug, Deserialize)]
struct ApiDiscount {
    #[serde(default)]
    is_discounted: bool,
    undiscounted_gross_price: Option<String>,
    discount_type: Option<String>,
    active_until: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiPromotion {
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiAvailability {
    is_available: bool,
}

/// Turns one product into a listing. `None` without a usable price.
fn parse_product(product: ApiProduct) -> Option<RawListing> {
    let current = price(&product.gross_price)?;
    let discount = product.discount.filter(|d| d.is_discounted);
    // A discount's undiscounted price is the shelf price; the current price is the offer.
    let shelf_price = discount
        .as_ref()
        .and_then(|d| d.undiscounted_gross_price.as_deref())
        .and_then(price)
        .filter(|undiscounted| *undiscounted >= current)
        .unwrap_or(current);
    let offer = discount.as_ref().and_then(|d| {
        let title = product.promotion.as_ref().and_then(|p| p.title.as_deref());
        let offer = offer(d.discount_type.as_deref(), title, current, shelf_price)?;
        Some(OfferInfo {
            offer,
            valid_from: None,
            valid_to: d.active_until.as_deref().and_then(oslo_date),
            source_flagged: true,
        })
    });

    let name_extra = product.name_extra.unwrap_or_default();
    let pack_size = parse_pack_size(&product.full_name)
        .or_else(|| parse_pack_size(&name_extra))
        .unwrap_or(1);
    Some(RawListing {
        source: SourceId::Oda,
        chain: Chain::Oda,
        source_product_id: product.id.to_string(),
        gtin: None,
        raw_name: product.full_name.trim().to_owned(),
        raw_brand: product.brand.filter(|b| !b.trim().is_empty()),
        raw_size: Some(name_extra).filter(|s| !s.trim().is_empty()),
        pack_size,
        shelf_price,
        member_price: None,
        offer,
        available: product.availability.map(|a| a.is_available),
        // Oda's search does not include the deposit; it is computed from the volume.
        deposit: None,
        // The fetch time is the observation time: Oda shows today's price.
        source_timestamp: None,
    })
}

/// "29.20" → `Ore(2920)`, without floats. Zero and unparseable prices are rejected.
fn price(text: &str) -> Option<Ore> {
    parse_kr(text).ok().filter(|p| p.0 > 0)
}

/// Maps an Oda discount to an offer (SPEC §4.6):
/// - `mix_and_match` "3 for 2" → n for m
/// - `fixed_price_bundle` "5 for 109 kr" → n for a sum
/// - `price_discount` (and any other lower price) → a fixed price
fn offer(kind: Option<&str>, title: Option<&str>, current: Ore, shelf: Ore) -> Option<Offer> {
    let bundle = title.and_then(parse_bundle);
    match (kind, bundle) {
        (Some("mix_and_match"), Some((n, Bundle::Count(m)))) if m < n => {
            Some(Offer::NForM { n, m })
        }
        (Some("fixed_price_bundle"), Some((n, Bundle::Kroner(sum)))) => {
            Some(Offer::NForSum { n, sum })
        }
        // An unknown type with a readable title: "3 for 2" is a count, "2 for 50" a sum.
        (_, Some((n, Bundle::Count(m)))) if m < n => Some(Offer::NForM { n, m }),
        (_, Some((n, Bundle::Kroner(sum)))) => Some(Offer::NForSum { n, sum }),
        _ if current < shelf => Some(Offer::FixedPrice { price: current }),
        _ => {
            tracing::debug!("oda: ukjent tilbud {kind:?} «{}»", title.unwrap_or(""));
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bundle {
    /// "3 for 2": pay for 2.
    Count(u32),
    /// "5 for 109 kr": pay 109 kr.
    Kroner(Ore),
}

/// Reads "<n> for <x>" from a campaign title: "3 for 2", "Miks 3 for 2", "5 for 109 kr".
/// A whole number smaller than `n` without "kr" is a count; anything else is kroner.
fn parse_bundle(title: &str) -> Option<(u32, Bundle)> {
    let cleaned = title.replace('\u{a0}', " ").to_lowercase();
    let words: Vec<&str> = cleaned.split_whitespace().collect();
    let at = words.iter().position(|w| *w == "for")?;
    let n: u32 = words.get(at.checked_sub(1)?)?.parse().ok()?;
    let x = words.get(at + 1)?.trim_end_matches([',', '-', '.']);
    let in_kroner = words.get(at + 2).is_some_and(|w| w.starts_with("kr"));
    if n < 2 {
        return None;
    }
    match x.parse::<u32>() {
        Ok(m) if !in_kroner && m < n => Some((n, Bundle::Count(m))),
        _ => Some((n, Bundle::Kroner(parse_kr(x).ok().filter(|s| s.0 > 0)?))),
    }
}

/// `active_until` as a date in Norway: a timestamp or a plain date.
fn oslo_date(text: &str) -> Option<Date> {
    if let Ok(timestamp) = text.parse::<Timestamp>() {
        return timestamp.in_tz("Europe/Oslo").ok().map(|z| z.date());
    }
    text.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE_1: &str = include_str!("../../tests/fixtures/oda/search-energidrikk-page1.json");
    const PAGE_2: &str = include_str!("../../tests/fixtures/oda/search-energidrikk-page2.json");

    fn listings(json: &str) -> (Vec<RawListing>, bool) {
        let page: Page = serde_json::from_str(json).unwrap();
        let more = page.attributes.has_more_items;
        let listings = page
            .items
            .into_iter()
            .filter(|i| i.kind == "product")
            .map(|i| serde_json::from_value::<ApiProduct>(i.attributes).unwrap())
            .filter_map(parse_product)
            .collect();
        (listings, more)
    }

    fn find<'a>(listings: &'a [RawListing], id: &str) -> &'a RawListing {
        listings.iter().find(|l| l.source_product_id == id).unwrap()
    }

    #[test]
    fn pages_are_parsed() {
        let (first, more) = listings(PAGE_1);
        assert!(more);
        assert_eq!(first.len(), 7);
        let (second, more) = listings(PAGE_2);
        assert!(!more);
        assert_eq!(second.len(), 3);
        assert!(
            first
                .iter()
                .all(|l| l.source == SourceId::Oda && l.chain == Chain::Oda)
        );
    }

    #[test]
    fn a_plain_product() {
        let (all, _) = listings(PAGE_1);
        let monster = find(&all, "23300");
        assert_eq!(monster.raw_name, "Monster Energy");
        assert_eq!(monster.raw_brand.as_deref(), Some("Monster"));
        assert_eq!(monster.raw_size.as_deref(), Some("0,5 l"));
        assert_eq!(monster.shelf_price, Ore(2920));
        assert_eq!(monster.offer, None);
        assert_eq!(monster.pack_size, 1);
        assert_eq!(monster.available, Some(true));
        assert_eq!(monster.gtin, None);
    }

    #[test]
    fn three_for_two_on_a_four_pack() {
        let (all, _) = listings(PAGE_1);
        let pack = find(&all, "66987");
        assert_eq!(pack.pack_size, 4);
        assert_eq!(pack.shelf_price, Ore(7990));
        let offer = pack.offer.as_ref().unwrap();
        assert_eq!(offer.offer, Offer::NForM { n: 3, m: 2 });
        assert!(offer.source_flagged);
    }

    #[test]
    fn five_for_a_sum() {
        let (all, _) = listings(PAGE_1);
        let battery = find(&all, "27093");
        assert_eq!(battery.shelf_price, Ore(2590));
        assert_eq!(
            battery.offer.as_ref().unwrap().offer,
            Offer::NForSum {
                n: 5,
                sum: Ore(10_900)
            }
        );
    }

    #[test]
    fn a_price_cut_keeps_the_old_price_as_shelf_price() {
        let (all, _) = listings(PAGE_1);
        let tray = find(&all, "67504");
        assert_eq!(tray.pack_size, 24);
        assert_eq!(tray.shelf_price, Ore(34_900));
        assert_eq!(
            tray.offer.as_ref().unwrap().offer,
            Offer::FixedPrice { price: Ore(29_900) }
        );
    }

    #[test]
    fn new_without_discount_is_not_an_offer_and_sold_out_is_unavailable() {
        let (all, _) = listings(PAGE_1);
        assert_eq!(find(&all, "69824").offer, None);
        // "available_later" ("Kan leveres 1. okt") can still be ordered.
        assert_eq!(find(&all, "31152").available, Some(true));
        let (second, _) = listings(PAGE_2);
        assert_eq!(find(&second, "67781").available, Some(false));
    }

    #[test]
    fn bundle_titles() {
        assert_eq!(parse_bundle("3 for 2"), Some((3, Bundle::Count(2))));
        assert_eq!(parse_bundle("Miks 3 for 2"), Some((3, Bundle::Count(2))));
        assert_eq!(
            parse_bundle("5 for 109\u{a0}kr"),
            Some((5, Bundle::Kroner(Ore(10_900))))
        );
        assert_eq!(
            parse_bundle("2 for 50,-"),
            Some((2, Bundle::Kroner(Ore(5_000))))
        );
        assert_eq!(
            parse_bundle("2 for 39,90"),
            Some((2, Bundle::Kroner(Ore(3_990))))
        );
        assert_eq!(parse_bundle("Salg!"), None);
        assert_eq!(parse_bundle("-40%"), None);
        assert_eq!(parse_bundle("1 for 10"), None);
    }

    #[test]
    fn offer_mapping() {
        let shelf = Ore(2590);
        assert_eq!(
            offer(Some("price_discount"), Some("-40%"), Ore(1674), Ore(2790)),
            Some(Offer::FixedPrice { price: Ore(1674) })
        );
        // A title that does not fit its type, and no lower price: no offer.
        assert_eq!(
            offer(Some("mix_and_match"), Some("Salg!"), shelf, shelf),
            None
        );
    }

    #[test]
    fn active_until_becomes_an_oslo_date() {
        assert_eq!(
            oslo_date("2026-10-04T22:30:00Z"),
            Some(jiff::civil::date(2026, 10, 5))
        );
        assert_eq!(
            oslo_date("2026-10-04"),
            Some(jiff::civil::date(2026, 10, 4))
        );
        assert_eq!(oslo_date("snart"), None);
    }

    #[test]
    fn page_url() {
        let oda = Oda::with_base_url("http://127.0.0.1:1234/api/v1/");
        assert_eq!(
            oda.page_url(2),
            "http://127.0.0.1:1234/api/v1/search/mixed/?q=energidrikk&type=product&page=2&size=50"
        );
    }
}
