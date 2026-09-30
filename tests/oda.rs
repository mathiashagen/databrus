//! The Oda adapter against a local mock of the API (wiremock) with real, trimmed responses
//! from `tests/fixtures/oda`. No test touches the network.

use std::process::Command;

use assert_cmd::assert::OutputAssertExt;
use databrus::sources::oda::Oda;
use databrus::sources::{FetchContext, Http, Source};
use predicates::prelude::*;
use tempfile::TempDir;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PAGE_1: &str = include_str!("fixtures/oda/search-energidrikk-page1.json");
const PAGE_2: &str = include_str!("fixtures/oda/search-energidrikk-page2.json");

fn json(text: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(text, "application/json")
}

async fn two_pages(server: &MockServer) {
    for (page, body) in [("1", PAGE_1), ("2", PAGE_2)] {
        Mock::given(method("GET"))
            .and(path("/api/v1/search/mixed/"))
            .and(query_param("q", "energidrikk"))
            .and(query_param("type", "product"))
            .and(query_param("size", "50"))
            .and(query_param("page", page))
            .respond_with(json(body))
            .expect(1)
            .mount(server)
            .await;
    }
}

#[tokio::test]
async fn fetches_both_pages_politely() {
    let server = MockServer::start().await;
    two_pages(&server).await;

    let ctx = FetchContext {
        http: Http::new().unwrap(),
        api_key: None,
        gtins: Vec::new(),
    };
    let listings = Oda::with_base_url(format!("{}/api/v1", server.uri()))
        .fetch(&ctx)
        .await
        .unwrap();
    assert_eq!(listings.len(), 10);

    // Oda's policy: the User-Agent says "bot", names the program and gives a contact.
    for request in server.received_requests().await.unwrap() {
        let ua = request.headers.get("user-agent").unwrap().to_str().unwrap();
        assert!(ua.starts_with("databrus-bot/"), "{ua}");
        assert!(ua.contains("github.com"), "{ua}");
        assert!(request.headers.get("authorization").is_none());
    }
}

/// The whole chain through the binary: fetch from Oda, match through source links and
/// names, and rank with Oda's campaigns as real offers. `--alle` keeps the test
/// independent of the fetch time.
#[tokio::test(flavor = "multi_thread")]
async fn update_and_search_with_campaigns() {
    let server = MockServer::start().await;
    two_pages(&server).await;
    let dir = TempDir::new().unwrap();
    let url = format!("{}/api/v1", server.uri());

    let databrus = move |dir: &TempDir, args: &[&str]| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_databrus"));
        command
            .args(args)
            .env("DATABRUS_KONFIG", dir.path().join("konfig.toml"))
            .env("DATABRUS_DATA_DIR", dir.path().join("data"))
            .env("DATABRUS_ODA_URL", &url)
            .env_remove("KASSALAPP_API_KEY")
            .env("NO_COLOR", "1");
        command
    };

    let (update, search, table, deals) = tokio::task::spawn_blocking(move || {
        let update = databrus(&dir, &["oppdater", "--kilde", "oda"]).assert();
        let search = databrus(
            &dir,
            &["--frakoblet", "--alle", "--json", "--butikk", "oda"],
        )
        .assert();
        let table = databrus(&dir, &["--frakoblet", "--alle", "monster", "ultra"]).assert();
        let deals = databrus(&dir, &["tilbud", "--frakoblet", "--alle", "--json"]).assert();
        (update, search, table, deals)
    })
    .await
    .unwrap();

    update
        .success()
        .stdout(predicate::str::contains("Oda: 10 oppføringer"));

    let output = search.success().get_output().stdout.clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let results = json["resultater"].as_array().unwrap();
    let by_id = |id: &str| {
        results
            .iter()
            .find(|r| r["produkt"]["id"] == id)
            .unwrap_or_else(|| panic!("{id} missing: {results:#?}"))
    };

    // Linked through `[[kildekobling]]`: a verified match without an EAN from Oda.
    let monster = by_id("monster-energy-500-boks");
    assert_eq!(monster["produkt"]["verifisert"], true);
    assert_eq!(monster["hyllepris_ore"], 2920);

    // "3 for 2" on a 4-pack: 79,90 × 2/3 / 4 = 13,32 per can, at least 12 cans.
    let ultra = by_id("monster-ultra-white-500-boks");
    assert_eq!(ultra["antall_i_pakke"], 4);
    assert_eq!(ultra["tilbud"]["type"], "n_for_m");
    assert_eq!(ultra["tilbudsmerke"], "KAMPANJE");
    assert_eq!(ultra["brukt_pris"], "tilbud");
    assert_eq!(ultra["effektiv_enhetspris_ore"], 1332);
    assert_eq!(ultra["minsteantall"], 12);

    // A price cut: 349 → 299 for 24 cans.
    let tray = results
        .iter()
        .find(|r| {
            r["produkt"]["id"] == "red-bull-energy-drink-250-boks" && r["antall_i_pakke"] == 24
        })
        .unwrap();
    assert_eq!(tray["hyllepris_ore"], 34_900);
    assert_eq!(tray["tilbud"]["type"], "fastpris");
    assert_eq!(tray["effektiv_enhetspris_ore"], 1246);

    // Sold out at the supplier.
    assert_eq!(
        by_id("monster-lando-norris-zs-500-boks")["tilgjengelig"],
        false
    );

    // The output validates against the published schema.
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../schema/v1.json")).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let errors: Vec<String> = validator
        .iter_errors(&json)
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");

    table
        .success()
        .stdout(predicate::str::contains("KAMPANJE 12stk"))
        .stdout(predicate::str::contains("4-pakning"));

    // `tilbud`: exactly the campaign rows, in the same schema-valid document. Without
    // history they are all UKJENT.
    let output = deals.success().get_output().stdout.clone();
    let deals: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let deal_rows = deals["resultater"].as_array().unwrap();
    let campaigns = results
        .iter()
        .filter(|r| r["tilbudsmerke"] == "KAMPANJE")
        .count();
    assert!(campaigns > 0);
    assert_eq!(deal_rows.len(), campaigns);
    assert!(
        deal_rows
            .iter()
            .all(|r| r["tilbudsmerke"] == "KAMPANJE" && r["vurdering"]["verdi"] == "UKJENT")
    );
    let errors: Vec<String> = validator
        .iter_errors(&deals)
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
}
