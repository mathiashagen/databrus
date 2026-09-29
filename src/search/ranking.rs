//! From stored prices to ranked rows (SPEC §5, §3.2).

use std::collections::HashMap;

use jiff::Timestamp;
use jiff::civil::Date;

use super::{SearchFilter, SortBy};
use crate::catalog::Catalog;
use crate::config::Config;
use crate::db::StoredPrice;
use crate::history::deals;
use crate::model::{
    MembershipProgram, Offer, Ore, PriceBasis, Product, ProductSummary, SearchResult, Verdict,
    VerdictInfo,
};
use crate::pricing::{self, PriceCalc, effective_unit_price};

/// Prices older than this are hidden without `--alle` (SPEC §8).
pub const MAX_AGE_HOURS: u32 = 14 * 24;

/// The result of a search.
#[derive(Debug, Clone, Default)]
pub struct SearchHits {
    /// Ranked rows, cut to `limit` without `--alle`.
    pub rows: Vec<SearchResult>,
    /// Hits before cutting.
    pub total: usize,
    pub hidden: Hidden,
}

/// Rows that matched the filters but were hidden without `--alle`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Hidden {
    pub stale: usize,
    pub sold_out: usize,
    pub suspicious: usize,
}

impl Hidden {
    pub fn total(&self) -> usize {
        self.stale + self.sold_out + self.suspicious
    }
}

/// Filters, prices and ranks. `now` and `today` are passed in, so the result is
/// predictable in tests.
pub fn rank(
    prices: &[StoredPrice],
    catalog: &Catalog,
    filter: &SearchFilter,
    config: &Config,
    now: Timestamp,
    today: Date,
) -> SearchHits {
    let mut hidden = Hidden::default();
    let mut rows = Vec::new();

    for price in prices {
        let Some(product) = catalog.find(&price.product) else {
            continue;
        };
        if !filter.chain_matches(price.chain) || !filter.product_matches(product) {
            continue;
        }

        let (calc, basis) = compute_price(price, filter.single_unit, &config.memberships, today);
        let liter_price = pricing::liter_price(calc.unit_price, product.volume);
        if filter.max_price.is_some_and(|m| calc.unit_price > m)
            || filter.max_liter_price.is_some_and(|m| liter_price > m)
        {
            continue;
        }

        let age_hours = u32::try_from(now.duration_since(price.last_seen).as_hours()).unwrap_or(0);
        let sold_out = price.available == Some(false);
        if !filter.all {
            if age_hours > MAX_AGE_HOURS {
                hidden.stale += 1;
                continue;
            }
            if sold_out {
                hidden.sold_out += 1;
                continue;
            }
            if price.suspicious {
                hidden.suspicious += 1;
                continue;
            }
        }

        rows.push(build_result(
            price,
            product,
            calc,
            basis,
            liter_price,
            age_hours,
            config,
            today,
        ));
    }

    if !filter.all {
        rows = cheapest_per_product_and_chain(rows);
    }
    sort_rows(&mut rows, filter.sort);

    let total = rows.len();
    if !filter.all {
        rows.truncate(filter.limit);
    }
    SearchHits {
        rows,
        total,
        hidden,
    }
}

/// The effective price per container, and which price was used (SPEC §5.2–5.3).
fn compute_price(
    price: &StoredPrice,
    single_unit: bool,
    memberships: &[MembershipProgram],
    today: Date,
) -> (PriceCalc, PriceBasis) {
    let offer = price
        .offer
        .as_ref()
        .filter(|o| deals::campaign_active(o, today))
        .map(|o| &o.offer)
        // `--enkeltvis`: only offers that apply when buying one.
        .filter(|o| !single_unit || matches!(o, Offer::FixedPrice { .. } | Offer::Percent { .. }));

    let mut calc = effective_unit_price(price.shelf_price, price.pack_size, offer);
    let mut basis = if calc.offer_applied {
        PriceBasis::Offer
    } else {
        PriceBasis::Shelf
    };

    if let Some(member) = price.member_price
        && memberships.contains(&member.program)
    {
        let as_member = effective_unit_price(member.price, price.pack_size, None);
        if as_member.unit_price < calc.unit_price {
            calc = as_member;
            basis = PriceBasis::Member;
        }
    }
    (calc, basis)
}

#[allow(clippy::too_many_arguments)]
fn build_result(
    price: &StoredPrice,
    product: &Product,
    calc: PriceCalc,
    price_basis: PriceBasis,
    liter_price: Ore,
    age_hours: u32,
    config: &Config,
    today: Date,
) -> SearchResult {
    let active_offer = price
        .offer
        .clone()
        .filter(|o| deals::campaign_active(o, today));
    let deposit = pricing::deposit(product.volume, &config.deposit);
    SearchResult {
        product: ProductSummary {
            id: product.id.clone(),
            name: product.name.clone(),
            brand: product.brand.clone(),
            flavor: product.flavor.clone(),
            sugar_free: product.sugar_free,
            volume: product.volume,
            container: product.container,
            store_brand: product.store_brand,
            verified: price.verified,
        },
        chain: price.chain,
        source: price.source,
        pack_size: price.pack_size,
        shelf_price: price.shelf_price,
        member_price: price.member_price.map(|m| m.price),
        membership_program: price.member_price.map(|m| m.program),
        effective_unit_price: calc.unit_price,
        liter_price,
        min_quantity: calc.min_quantity,
        price_basis,
        deposit,
        deposit_min_quantity: Ore(deposit.0 * i64::from(calc.min_quantity)),
        deal_badge: deals::deal_badge(active_offer.as_ref(), today),
        offer: active_offer,
        // Verdicts and reference prices come with the history in M2.
        verdict: VerdictInfo {
            value: Verdict::Unknown,
            l30_ore: None,
            m90_ore: None,
            atl_ore: None,
            coverage_days: 0,
        },
        price_range: None,
        available: price.available != Some(false),
        last_seen: price.last_seen,
        age_hours,
    }
}

/// Without `--alle`, only the cheapest listing per (product, chain) is shown – e.g.
/// either the single can or the 4-pack, not both (SPEC §5.2).
fn cheapest_per_product_and_chain(rows: Vec<SearchResult>) -> Vec<SearchResult> {
    let mut best: HashMap<_, SearchResult> = HashMap::new();
    for row in rows {
        let key = (row.product.id.clone(), row.chain);
        match best.get(&key) {
            Some(current)
                if (current.liter_price, current.min_quantity)
                    <= (row.liter_price, row.min_quantity) => {}
            _ => {
                best.insert(key, row);
            }
        }
    }
    best.into_values().collect()
}

fn sort_rows(rows: &mut [SearchResult], sort: SortBy) {
    // Stable order for equal prices: name, size, chain.
    let name = |r: &SearchResult| (r.product.name.to_lowercase(), r.product.volume, r.chain);
    match sort {
        // Discount needs the 90-day median (M2); until then it sorts like liter price.
        SortBy::LiterPrice | SortBy::Discount => {
            rows.sort_by_cached_key(|r| (r.liter_price, r.effective_unit_price, name(r)));
        }
        SortBy::Price => {
            rows.sort_by_cached_key(|r| (r.effective_unit_price, r.liter_price, name(r)));
        }
        SortBy::Name => rows.sort_by_cached_key(|r| (name(r), r.liter_price)),
    }
}

#[cfg(test)]
mod tests {
    use jiff::ToSpan;
    use jiff::civil::date;

    use super::*;
    use crate::cli::{ChainChoice, SearchArgs};
    use crate::model::{Chain, DealBadge, MemberPrice, OfferInfo, ProductId, SourceId};

    const CATALOG: &str = r#"
        [[produkt]]
        id = "monster-ultra-white-500-boks"
        navn = "Monster Ultra White"
        merke = "monster"
        smak = "ultra-white"
        sukkerfri = true
        volum_ml = 500
        beholder = "boks"

        [[produkt]]
        id = "red-bull-energy-drink-250-boks"
        navn = "Red Bull Energy Drink"
        merke = "red-bull"
        smak = "original"
        sukkerfri = false
        volum_ml = 250
        beholder = "boks"
    "#;

    const WHITE: &str = "monster-ultra-white-500-boks";
    const RED_BULL: &str = "red-bull-energy-drink-250-boks";

    fn now() -> Timestamp {
        "2026-09-29T12:00:00Z".parse().unwrap()
    }

    fn today() -> Date {
        date(2026, 9, 29)
    }

    fn price(id: i64, product: &str, chain: Chain, shelf_price: i64) -> StoredPrice {
        StoredPrice {
            listing_id: id,
            source: SourceId::Kassalapp,
            chain,
            product: ProductId(product.into()),
            pack_size: 1,
            verified: true,
            shelf_price: Ore(shelf_price),
            member_price: None,
            offer: None,
            available: None,
            suspicious: false,
            last_seen: now() - 2.hours(),
        }
    }

    fn search(prices: &[StoredPrice], args: SearchArgs, config: &Config) -> SearchHits {
        let catalog = Catalog::from_toml(CATALOG).unwrap();
        let filter = SearchFilter::from_args(&args, config);
        rank(prices, &catalog, &filter, config, now(), today())
    }

    fn chains(hits: &SearchHits) -> Vec<(Chain, i64)> {
        hits.rows
            .iter()
            .map(|r| (r.chain, r.liter_price.0))
            .collect()
    }

    #[test]
    fn sorted_by_liter_price_with_deposit_separate() {
        let prices = [
            price(1, WHITE, Chain::Meny, 3290),
            price(2, WHITE, Chain::Spar, 1690),
            price(3, RED_BULL, Chain::Joker, 2490),
        ];
        let hits = search(&prices, SearchArgs::default(), &Config::default());
        // 16,90/0,5 = 33,80 · 32,90/0,5 = 65,80 · 24,90/0,25 = 99,60
        assert_eq!(
            chains(&hits),
            [
                (Chain::Spar, 3380),
                (Chain::Meny, 6580),
                (Chain::Joker, 9960)
            ]
        );
        let spar = &hits.rows[0];
        assert_eq!(spar.deposit, Ore(200));
        assert_eq!(spar.effective_unit_price, Ore(1690));
        assert_eq!(spar.price_basis, PriceBasis::Shelf);
        assert_eq!(spar.age_hours, 2);
    }

    #[test]
    fn filters_on_chain_and_liter_price() {
        let prices = [
            price(1, WHITE, Chain::Meny, 3290),
            price(2, WHITE, Chain::Spar, 1690),
            price(3, RED_BULL, Chain::Spar, 2490),
        ];
        let args = SearchArgs {
            stores: vec![ChainChoice::One(Chain::Spar)],
            max_liter_price: Some(Ore::from_kr(50)),
            ..SearchArgs::default()
        };
        let hits = search(&prices, args, &Config::default());
        assert_eq!(chains(&hits), [(Chain::Spar, 3380)]);
    }

    #[test]
    fn stale_prices_are_hidden_without_all() {
        let mut stale = price(1, WHITE, Chain::Kiwi, 1990);
        stale.last_seen = now() - (15 * 24).hours();
        let prices = [stale, price(2, WHITE, Chain::Meny, 3290)];

        let hits = search(&prices, SearchArgs::default(), &Config::default());
        assert_eq!(chains(&hits), [(Chain::Meny, 6580)]);
        assert_eq!(hits.hidden.stale, 1);

        let all = SearchArgs {
            all: true,
            ..SearchArgs::default()
        };
        let hits = search(&prices, all, &Config::default());
        assert_eq!(hits.rows.len(), 2);
        assert_eq!(hits.rows[0].age_hours, 15 * 24);
    }

    #[test]
    fn multipack_is_priced_per_can_and_the_cheapest_is_shown() {
        let single = price(1, WHITE, Chain::Joker, 2990);
        let mut four_pack = price(2, WHITE, Chain::Joker, 9960);
        four_pack.pack_size = 4;
        let prices = [single, four_pack];

        let hits = search(&prices, SearchArgs::default(), &Config::default());
        assert_eq!(hits.rows.len(), 1);
        let row = &hits.rows[0];
        assert_eq!(row.effective_unit_price, Ore(2490));
        assert_eq!(row.min_quantity, 4);
        assert_eq!(row.deposit_min_quantity, Ore(800));

        let all = SearchArgs {
            all: true,
            ..SearchArgs::default()
        };
        assert_eq!(search(&prices, all, &Config::default()).rows.len(), 2);
    }

    #[test]
    fn active_multibuy_offer_is_used_except_single_unit() {
        let mut offered = price(1, WHITE, Chain::Kiwi, 2490);
        offered.offer = Some(OfferInfo {
            offer: Offer::NForM { n: 3, m: 2 },
            valid_from: Some(date(2026, 9, 28)),
            valid_to: Some(date(2026, 10, 4)),
            source_flagged: true,
        });
        let prices = [offered];

        let row = &search(&prices, SearchArgs::default(), &Config::default()).rows[0];
        assert_eq!(row.effective_unit_price, Ore(1660));
        assert_eq!(row.min_quantity, 3);
        assert_eq!(row.price_basis, PriceBasis::Offer);
        assert_eq!(row.deal_badge, Some(DealBadge::Campaign));

        let single_unit = SearchArgs {
            single_unit: true,
            ..SearchArgs::default()
        };
        let row = &search(&prices, single_unit, &Config::default()).rows[0];
        assert_eq!(row.effective_unit_price, Ore(2490));
        assert_eq!(row.min_quantity, 1);
    }

    #[test]
    fn expired_offer_is_ignored() {
        let mut expired = price(1, WHITE, Chain::Kiwi, 2490);
        expired.offer = Some(OfferInfo {
            offer: Offer::FixedPrice { price: Ore(1590) },
            valid_from: None,
            valid_to: Some(date(2026, 9, 27)),
            source_flagged: true,
        });
        let row = &search(&[expired], SearchArgs::default(), &Config::default()).rows[0];
        assert_eq!(row.effective_unit_price, Ore(2490));
        assert_eq!(row.offer, None);
        assert_eq!(row.deal_badge, None);
    }

    #[test]
    fn member_price_only_for_members() {
        let mut coop = price(1, WHITE, Chain::CoopExtra, 2490);
        coop.member_price = Some(MemberPrice {
            price: Ore(1990),
            program: MembershipProgram::Coop,
        });
        let prices = [coop];

        let row = &search(&prices, SearchArgs::default(), &Config::default()).rows[0];
        assert_eq!(row.effective_unit_price, Ore(2490));
        assert_eq!(row.member_price, Some(Ore(1990)));

        let member = Config {
            memberships: vec![MembershipProgram::Coop],
            ..Config::default()
        };
        let row = &search(&prices, SearchArgs::default(), &member).rows[0];
        assert_eq!(row.effective_unit_price, Ore(1990));
        assert_eq!(row.price_basis, PriceBasis::Member);
    }

    #[test]
    fn sold_out_and_suspicious_are_hidden_without_all() {
        let mut sold_out = price(1, WHITE, Chain::Oda, 2190);
        sold_out.available = Some(false);
        let mut suspicious = price(2, WHITE, Chain::Meny, 290);
        suspicious.suspicious = true;
        let hits = search(
            &[sold_out, suspicious],
            SearchArgs::default(),
            &Config::default(),
        );
        assert!(hits.rows.is_empty());
        assert_eq!(
            hits.hidden,
            Hidden {
                stale: 0,
                sold_out: 1,
                suspicious: 1
            }
        );
    }

    #[test]
    fn cut_to_the_default_limit() {
        let prices: Vec<_> = Chain::ALL
            .iter()
            .enumerate()
            .map(|(i, c)| price(i as i64, WHITE, *c, 2000 + i as i64))
            .collect();
        let config = Config {
            default_limit: 3,
            ..Config::default()
        };
        let hits = search(&prices, SearchArgs::default(), &config);
        assert_eq!(hits.rows.len(), 3);
        assert_eq!(hits.total, Chain::ALL.len());
    }

    #[test]
    fn sort_by_price_and_name() {
        let prices = [
            price(1, WHITE, Chain::Meny, 2290),
            price(2, RED_BULL, Chain::Spar, 1990),
        ];
        let by_price = SearchArgs {
            sort: SortBy::Price,
            ..SearchArgs::default()
        };
        let hits = search(&prices, by_price, &Config::default());
        assert_eq!(hits.rows[0].product.name, "Red Bull Energy Drink");

        let by_name = SearchArgs {
            sort: SortBy::Name,
            ..SearchArgs::default()
        };
        let hits = search(&prices, by_name, &Config::default());
        assert_eq!(hits.rows[0].product.name, "Monster Ultra White");
    }

    /// The JSON contract (SPEC §9.1): the keys are Norwegian even though the fields are
    /// English.
    #[test]
    fn result_json_keys_are_norwegian() {
        let hits = search(
            &[price(1, WHITE, Chain::Kiwi, 2490)],
            SearchArgs::default(),
            &Config::default(),
        );
        let json = serde_json::to_value(&hits.rows[0]).unwrap();
        let keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let mut expected = vec![
            "produkt",
            "kjede",
            "kilde",
            "antall_i_pakke",
            "hyllepris_ore",
            "medlemspris_ore",
            "medlemsprogram",
            "effektiv_enhetspris_ore",
            "literpris_ore",
            "minsteantall",
            "brukt_pris",
            "pant_ore",
            "pant_minsteantall_ore",
            "tilbud",
            "tilbudsmerke",
            "vurdering",
            "prisspenn",
            "tilgjengelig",
            "sist_sett",
            "alder_timer",
        ];
        let mut keys_sorted = keys.clone();
        keys_sorted.sort_unstable();
        expected.sort_unstable();
        assert_eq!(keys_sorted, expected);

        let mut product: Vec<&str> = json["produkt"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        product.sort_unstable();
        assert_eq!(
            product,
            [
                "beholder",
                "egenmerke",
                "id",
                "merke",
                "navn",
                "smak",
                "sukkerfri",
                "verifisert",
                "volum_ml"
            ]
        );
        assert_eq!(json["brukt_pris"], "hyllepris");
        assert_eq!(json["vurdering"]["verdi"], "UKJENT");
        assert!(json["vurdering"].get("dekning_dager").is_some());
    }
}
