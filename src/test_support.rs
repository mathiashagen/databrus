//! Shared test data: a small catalog and a set of search hits that covers every column and
//! badge (an offer, a member price, a pack, an unverified match and a stale row).

use jiff::civil::date;
use jiff::{Timestamp, ToSpan};

use crate::catalog::Catalog;
use crate::cli::SearchArgs;
use crate::config::Config;
use crate::db::StoredPrice;
use crate::model::{
    Chain, MemberPrice, MembershipProgram, Offer, OfferInfo, Ore, ProductId, SourceId,
};
use crate::search::SearchFilter;
use crate::search::ranking::{SearchHits, rank};

pub(crate) const CATALOG: &str = r#"
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

pub(crate) fn now() -> Timestamp {
    "2026-09-29T12:00:00Z".parse().unwrap()
}

pub(crate) fn price(product: &str, chain: Chain, shelf_price: i64, hours_ago: i64) -> StoredPrice {
    StoredPrice {
        listing_id: 0,
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
        last_seen: now() - hours_ago.hours(),
    }
}

pub(crate) fn example_hits() -> SearchHits {
    let mut campaign = price("monster-ultra-white-500-boks", Chain::Kiwi, 2490, 2);
    campaign.offer = Some(OfferInfo {
        offer: Offer::NForSum {
            n: 3,
            sum: Ore(4770),
        },
        valid_from: Some(date(2026, 9, 28)),
        valid_to: Some(date(2026, 10, 4)),
        source_flagged: true,
    });
    let mut member = price("monster-ultra-white-500-boks", Chain::CoopExtra, 2190, 3);
    member.member_price = Some(MemberPrice {
        price: Ore(1890),
        program: MembershipProgram::Coop,
    });
    let mut pack = price("red-bull-energy-drink-250-boks", Chain::Joker, 9960, 72);
    pack.pack_size = 4;
    let mut unverified = price("monster-ultra-white-500-boks", Chain::Oda, 2240, 5);
    unverified.verified = false;
    let stale = price(
        "red-bull-energy-drink-250-boks",
        Chain::Rema,
        1990,
        24 * 400,
    );

    let catalog = Catalog::from_toml(CATALOG).unwrap();
    let filter = SearchFilter::from_args(&SearchArgs::default(), &Config::default());
    rank(
        &[campaign, member, pack, unverified, stale],
        &catalog,
        &filter,
        &Config::default(),
        now(),
        date(2026, 9, 29),
    )
}
