//! The Kassalapp adapter against a local mock of the API (wiremock) with real, trimmed
//! responses from `tests/fixtures/kassalapp`. No test touches the network.

use std::process::Command;

use assert_cmd::assert::OutputAssertExt;
use databrus::sources::kassalapp::Kassalapp;
use databrus::sources::{FetchContext, Http, Source, SourceError};
use predicates::prelude::*;
use tempfile::TempDir;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PAGE_1: &str = include_str!("fixtures/kassalapp/category-energy-drinks.json");
const LAST_PAGE: &str = include_str!("fixtures/kassalapp/category-energy-drinks-last-page.json");

fn json(text: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(text, "application/json")
}

fn context(api_key: Option<&str>) -> FetchContext {
    FetchContext {
        http: Http::new().unwrap(),
        api_key: api_key.map(str::to_owned),
        gtins: Vec::new(),
    }
}

fn source(server: &MockServer) -> Kassalapp {
    Kassalapp::with_base_url(format!("{}/api/v1", server.uri()))
}

/// Page 1 has `next`, page 2 does not – like the real category.
async fn two_pages(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/v1/products"))
        .and(query_param("category_id", "111"))
        .and(query_param("size", "100"))
        .and(query_param("page", "1"))
        .and(header("authorization", "Bearer testnokkel"))
        .respond_with(json(PAGE_1))
        .expect(1)
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/products"))
        .and(query_param("page", "2"))
        .and(header("authorization", "Bearer testnokkel"))
        .respond_with(json(LAST_PAGE))
        .expect(1)
        .mount(server)
        .await;
}

#[tokio::test]
async fn fetches_all_pages() {
    let server = MockServer::start().await;
    two_pages(&server).await;

    let listings = source(&server)
        .fetch(&context(Some("testnokkel")))
        .await
        .unwrap();
    // 12 from page 1 (2 Coop rows are skipped) and 2 from the last page.
    assert_eq!(listings.len(), 14);
    assert!(
        listings
            .iter()
            .any(|l| l.source_product_id == "225194" && l.pack_size == 4)
    );
}

#[tokio::test]
async fn retries_after_a_server_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(query_param("page", "1"))
        .respond_with(json(LAST_PAGE))
        .expect(1)
        .mount(&server)
        .await;

    let listings = source(&server)
        .fetch(&context(Some("testnokkel")))
        .await
        .unwrap();
    assert_eq!(listings.len(), 2);
}

#[tokio::test]
async fn gives_up_after_three_retries() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
        .expect(4)
        .mount(&server)
        .await;

    let error = source(&server)
        .fetch(&context(Some("testnokkel")))
        .await
        .unwrap_err();
    assert!(
        matches!(error, SourceError::Http { status: 429 }),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_rejected_key_is_not_retried() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;

    let error = source(&server)
        .fetch(&context(Some("feil")))
        .await
        .unwrap_err();
    assert!(
        matches!(error, SourceError::RejectedKey { status: 401 }),
        "{error:?}"
    );
}

#[tokio::test]
async fn nothing_is_sent_without_a_key() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(json(PAGE_1))
        .expect(0)
        .mount(&server)
        .await;

    let error = source(&server).fetch(&context(None)).await.unwrap_err();
    assert!(matches!(error, SourceError::MissingApiKey));
}

#[tokio::test]
async fn a_changed_response_format_is_a_schema_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(json(r#"{"data": "not a list"}"#))
        .mount(&server)
        .await;

    let error = source(&server)
        .fetch(&context(Some("testnokkel")))
        .await
        .unwrap_err();
    assert!(matches!(error, SourceError::SchemaChange(_)), "{error:?}");
}

/// The whole chain through the binary: `oppdater` fetches, matches against the built-in
/// catalog and stores, and an offline search ranks the result. Every fixture row has an
/// EAN that is in the catalog. The search uses `--alle`, so the test does not depend on
/// how old the fixture timestamps are when it runs.
#[tokio::test(flavor = "multi_thread")]
async fn update_and_search() {
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
            .env("DATABRUS_KASSALAPP_URL", &url)
            .env("KASSALAPP_API_KEY", "testnokkel")
            .env("NO_COLOR", "1");
        command
    };

    let (update, unmatched, json_search, table_search) = tokio::task::spawn_blocking(move || {
        let update = databrus(&dir, &["oppdater", "--kilde", "kassalapp"]).assert();
        let unmatched = databrus(&dir, &["produkter", "--ukjente", "--json"]).assert();
        let json_search = databrus(&dir, &["--frakoblet", "--alle", "--json", "monster"]).assert();
        let table_search = databrus(&dir, &["--frakoblet", "--alle", "monster", "mango"]).assert();
        (update, unmatched, json_search, table_search)
    })
    .await
    .unwrap();

    update.success().stdout(predicate::str::contains(
        "Kassalapp: 14 oppføringer, 14 matchet katalogen",
    ));
    let output = unmatched.success().get_output().stdout.clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["ukjente"].as_array().unwrap().len(), 0);

    // The last fixture page has two Monster 4-packs from Joker.
    let output = json_search.success().get_output().stdout.clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["skjemaversjon"], 1);
    assert_eq!(json["sporring"]["tekst"], "monster");
    assert_eq!(json["sporring"]["filtre"]["alle"], true);
    let results = json["resultater"].as_array().unwrap();
    assert_eq!(results.len(), 2, "{results:#?}");
    let mango = results
        .iter()
        .find(|r| r["produkt"]["id"] == "monster-mango-loco-500-boks")
        .unwrap();
    assert_eq!(mango["kjede"], "joker");
    assert_eq!(mango["antall_i_pakke"], 4);
    assert_eq!(mango["hyllepris_ore"], 9960);
    assert_eq!(mango["effektiv_enhetspris_ore"], 2490);
    assert_eq!(mango["literpris_ore"], 4980);
    assert_eq!(mango["minsteantall"], 4);
    assert_eq!(mango["pant_ore"], 200);
    assert_eq!(mango["brukt_pris"], "hyllepris");
    assert_eq!(mango["vurdering"]["verdi"], "UKJENT");
    assert_eq!(json["kilder"][0]["id"], "kassalapp");
    assert_eq!(json["kilder"][0]["status"], "ok");

    // The real output validates against the published schema.
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../schema/v1.json")).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let errors: Vec<String> = validator
        .iter_errors(&json)
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");

    table_search
        .success()
        .stdout(predicate::str::contains("Monster Mango Loco"))
        .stdout(predicate::str::contains("24,90"))
        .stdout(predicate::str::contains("4-pakning"))
        .stdout(predicate::str::contains("Viser 1 av 1 treff"));
}
