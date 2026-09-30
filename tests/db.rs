//! The database contract: migrations and change-only price history (SPEC §7.2).

use databrus::catalog::Catalog;
use databrus::catalog::matching::Matcher;
use databrus::db::{Change, Database, PriceObservation};
use databrus::model::{Chain, Offer, OfferInfo, Ore, RawListing, SourceId};
use jiff::{Timestamp, ToSpan};
use tempfile::TempDir;

fn open(dir: &TempDir) -> Database {
    Database::open(&dir.path().join("subdir").join("databrus.sqlite")).unwrap()
}

fn listing(shelf_price: i64) -> RawListing {
    RawListing {
        source: SourceId::Kassalapp,
        chain: Chain::Kiwi,
        source_product_id: "12345".into(),
        gtin: None,
        raw_name: "Monster Energy Ultra White 0,5l".into(),
        raw_brand: Some("Monster".into()),
        raw_size: Some("0,5l".into()),
        pack_size: 1,
        shelf_price: Ore(shelf_price),
        member_price: None,
        offer: None,
        available: Some(true),
        deposit: None,
        source_timestamp: None,
    }
}

fn at(hours: i64) -> Timestamp {
    Timestamp::from_second(1_790_000_000).unwrap() + hours.hours()
}

#[test]
fn migration_creates_an_empty_database() {
    let dir = TempDir::new().unwrap();
    let db = open(&dir);
    assert!(!db.has_price_data().unwrap());
    assert_eq!(db.last_fetch(SourceId::Kassalapp).unwrap(), None);
    // Opening again does not rerun the migrations.
    drop(db);
    open(&dir);
}

#[test]
fn same_price_only_updates_last_seen() {
    let dir = TempDir::new().unwrap();
    let mut db = open(&dir);
    let raw = listing(2490);

    let id = db.save_listing(&raw, None, at(0)).unwrap();
    let first = db
        .record_price(id, &PriceObservation::from_listing(&raw), at(0))
        .unwrap();
    assert!(matches!(first, Change::NewInterval(_)));

    // The same listing gives the same id, and the same price gives no new row.
    let id_again = db.save_listing(&raw, None, at(6)).unwrap();
    assert_eq!(id, id_again);
    let second = db
        .record_price(id, &PriceObservation::from_listing(&raw), at(6))
        .unwrap();
    assert_eq!(second, Change::Unchanged);
    assert_eq!(db.price_interval_count().unwrap(), 1);
    assert!(db.has_price_data().unwrap());
}

#[test]
fn last_seen_never_goes_backwards() {
    let dir = TempDir::new().unwrap();
    let mut db = open(&dir);
    let raw = listing(2490);
    let id = db.save_listing(&raw, None, at(0)).unwrap();
    db.record_price(id, &PriceObservation::from_listing(&raw), at(24))
        .unwrap();
    assert_eq!(
        db.newest_price_per_chain().unwrap().get(&Chain::Kiwi),
        Some(&at(24))
    );
    // Same price, but the source reports an older timestamp.
    let change = db
        .record_price(id, &PriceObservation::from_listing(&raw), at(2))
        .unwrap();
    assert_eq!(change, Change::Unchanged);
    assert_eq!(db.price_last_seen(id).unwrap(), Some(at(24)));
}

#[test]
fn a_new_price_or_offer_starts_a_new_interval() {
    let dir = TempDir::new().unwrap();
    let mut db = open(&dir);
    let raw = listing(2490);
    let id = db.save_listing(&raw, None, at(0)).unwrap();
    db.record_price(id, &PriceObservation::from_listing(&raw), at(0))
        .unwrap();

    let cheaper = listing(2290);
    let change = db
        .record_price(id, &PriceObservation::from_listing(&cheaper), at(6))
        .unwrap();
    assert!(matches!(change, Change::NewInterval(_)));

    let mut with_offer = listing(2290);
    with_offer.offer = Some(OfferInfo {
        offer: Offer::NForM { n: 3, m: 2 },
        valid_from: None,
        valid_to: None,
        source_flagged: true,
    });
    let change = db
        .record_price(id, &PriceObservation::from_listing(&with_offer), at(12))
        .unwrap();
    assert!(matches!(change, Change::NewInterval(_)));

    // Going back to an earlier price is also a change.
    let change = db
        .record_price(id, &PriceObservation::from_listing(&raw), at(18))
        .unwrap();
    assert!(matches!(change, Change::NewInterval(_)));
    assert_eq!(db.price_interval_count().unwrap(), 4);

    // The stored offer round-trips with its Norwegian JSON keys.
    let latest = db.latest_prices().unwrap();
    assert!(
        latest.is_empty(),
        "unmatched listings are not search results"
    );
}

#[test]
fn matched_listing_points_to_the_catalog_product() {
    let dir = TempDir::new().unwrap();
    let mut db = open(&dir);
    let catalog = Catalog::from_toml(
        r#"
        [[produkt]]
        id = "monster-ultra-white-500-boks"
        navn = "Monster Ultra White"
        merke = "monster"
        smak = "white"
        sukkerfri = true
        volum_ml = 500
        beholder = "boks"

        [[flerpakning]]
        gtin = "7040110569915"
        produkt = "monster-ultra-white-500-boks"
        antall = 4
        "#,
    )
    .unwrap();
    db.sync_catalog(&catalog).unwrap();

    let mut pack = listing(6990);
    pack.source_product_id = "pack-1".into();
    pack.gtin = Some("7040110569915".into());
    let found = Matcher::build(&catalog).find(&pack).unwrap();
    assert_eq!(found.pack_size, 4);
    db.save_listing(&pack, Some(&found), at(0)).unwrap();

    // Unmatched listings are listed for `produkter --ukjente`.
    db.save_listing(&listing(2490), None, at(0)).unwrap();
    let unmatched = db.unmatched_listings().unwrap();
    assert_eq!(unmatched.len(), 1);
    assert_eq!(unmatched[0].source_product_id, "12345");
}

/// Engrossnett sells trays under the single-can EAN, and other stores may use a pack EAN
/// for a single can. The pack size must come from the listing's name.
#[test]
fn pack_size_comes_from_the_name_not_the_ean() {
    let dir = TempDir::new().unwrap();
    let mut db = open(&dir);
    let catalog = Catalog::from_toml(
        r#"
        [[produkt]]
        id = "monster-energy-500-boks"
        navn = "Monster Energy"
        merke = "monster"
        smak = "original"
        sukkerfri = false
        volum_ml = 500
        beholder = "boks"
        gtin = ["5060166693732"]

        [[flerpakning]]
        gtin = "5060517881412"
        produkt = "monster-energy-500-boks"
        antall = 4
        "#,
    )
    .unwrap();
    db.sync_catalog(&catalog).unwrap();
    let matcher = Matcher::build(&catalog);

    // A tray under the single-can EAN.
    let mut tray = listing(73_531);
    tray.chain = Chain::Engrossnett;
    tray.source_product_id = "tray".into();
    tray.raw_name = "Monster Energy 24x500ml".into();
    tray.pack_size = 24;
    tray.gtin = Some("5060166693732".into());
    // A single can under the pack EAN.
    let mut single = listing(3090);
    single.chain = Chain::Oda;
    single.source_product_id = "single".into();
    single.raw_name = "Monster Energy 0,5 l".into();
    single.gtin = Some("5060517881412".into());

    for raw in [&tray, &single] {
        let found = matcher.find(raw).unwrap();
        let id = db.save_listing(raw, Some(&found), at(0)).unwrap();
        db.record_price(id, &PriceObservation::from_listing(raw), at(0))
            .unwrap();
    }
    let mut pack_sizes: Vec<(Chain, u32)> = db
        .latest_prices()
        .unwrap()
        .into_iter()
        .map(|p| (p.chain, p.pack_size))
        .collect();
    pack_sizes.sort();
    assert_eq!(pack_sizes, [(Chain::Oda, 1), (Chain::Engrossnett, 24)]);
}

#[test]
fn fetch_log() {
    let dir = TempDir::new().unwrap();
    let db = open(&dir);
    db.log_fetch(SourceId::Oda, at(0), at(0), Ok(42)).unwrap();
    db.log_fetch(SourceId::Oda, at(1), at(1), Err("HTTP 503"))
        .unwrap();

    let last = db.last_fetch(SourceId::Oda).unwrap().unwrap();
    assert!(!last.ok);
    assert_eq!(last.error_message.as_deref(), Some("HTTP 503"));
    assert_eq!(
        db.last_successful_fetch(SourceId::Oda).unwrap(),
        Some(at(0))
    );
    assert_eq!(db.last_fetch(SourceId::Coop).unwrap(), None);
}

/// The history of matched listings, oldest first, with each interval's start.
#[test]
fn price_history_lists_every_interval_in_order() {
    let dir = TempDir::new().unwrap();
    let mut db = open(&dir);
    let catalog = Catalog::from_toml(
        r#"
        [[produkt]]
        id = "monster-ultra-white-500-boks"
        navn = "Monster Ultra White"
        merke = "monster"
        smak = "white"
        sukkerfri = true
        volum_ml = 500
        beholder = "boks"
        "#,
    )
    .unwrap();
    db.sync_catalog(&catalog).unwrap();

    let raw = listing(2490);
    let found = Matcher::build(&catalog).find(&raw).unwrap();
    let id = db.save_listing(&raw, Some(&found), at(0)).unwrap();
    for (price, hours) in [(2490, 0), (2490, 24), (1990, 48), (2490, 72)] {
        db.record_price(
            id,
            &PriceObservation::from_listing(&listing(price)),
            at(hours),
        )
        .unwrap();
    }
    // An unmatched listing has no history in the search.
    let mut other = listing(990);
    other.source_product_id = "other".into();
    other.raw_name = "Ukjent drikk".into();
    let other_id = db.save_listing(&other, None, at(0)).unwrap();
    db.record_price(other_id, &PriceObservation::from_listing(&other), at(0))
        .unwrap();

    let history = db.price_history().unwrap();
    let rows: Vec<_> = history
        .iter()
        .map(|h| (h.price.shelf_price.0, h.valid_from, h.price.last_seen))
        .collect();
    assert_eq!(
        rows,
        [
            (2490, at(0), at(24)),
            (1990, at(48), at(48)),
            (2490, at(72), at(72)),
        ]
    );
}
