//! Kassalapp-adapteren mot en lokal etterligning av API-et (wiremock) med ekte, nedkortede
//! svar fra `tests/fixtures/kassalapp`. Ingen test treffer nettverket.

use std::process::Command;

use assert_cmd::assert::OutputAssertExt;
use databrus::kilder::kassalapp::Kassalapp;
use databrus::kilder::{HenteKontekst, Http, Kilde, KildeFeil};
use predicates::prelude::*;
use tempfile::TempDir;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SIDE_1: &str = include_str!("fixtures/kassalapp/kategori-energidrikk.json");
const SISTE_SIDE: &str = include_str!("fixtures/kassalapp/kategori-energidrikk-siste-side.json");

fn json(tekst: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(tekst, "application/json")
}

fn kontekst(nokkel: Option<&str>) -> HenteKontekst {
    HenteKontekst {
        http: Http::ny().unwrap(),
        api_nokkel: nokkel.map(str::to_owned),
        gtin: Vec::new(),
    }
}

fn kilde(server: &MockServer) -> Kassalapp {
    Kassalapp::med_basis_url(format!("{}/api/v1", server.uri()))
}

/// Side 1 har `next`, side 2 har ikke – som den ekte kategorien.
async fn to_sider(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/v1/products"))
        .and(query_param("category_id", "111"))
        .and(query_param("size", "100"))
        .and(query_param("page", "1"))
        .and(header("authorization", "Bearer testnokkel"))
        .respond_with(json(SIDE_1))
        .expect(1)
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/products"))
        .and(query_param("page", "2"))
        .and(header("authorization", "Bearer testnokkel"))
        .respond_with(json(SISTE_SIDE))
        .expect(1)
        .mount(server)
        .await;
}

#[tokio::test]
async fn henter_alle_sider() {
    let server = MockServer::start().await;
    to_sider(&server).await;

    let rader = kilde(&server)
        .hent(&kontekst(Some("testnokkel")))
        .await
        .unwrap();
    // 12 fra side 1 (2 Coop-rader hoppes over) og 2 fra siste side.
    assert_eq!(rader.len(), 14);
    assert!(
        rader
            .iter()
            .any(|r| r.kilde_produkt_id == "225194" && r.antall == 4)
    );
}

#[tokio::test]
async fn nytt_forsok_etter_serverfeil() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(query_param("page", "1"))
        .respond_with(json(SISTE_SIDE))
        .expect(1)
        .mount(&server)
        .await;

    let rader = kilde(&server)
        .hent(&kontekst(Some("testnokkel")))
        .await
        .unwrap();
    assert_eq!(rader.len(), 2);
}

#[tokio::test]
async fn gir_opp_etter_tre_nye_forsok() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
        .expect(4)
        .mount(&server)
        .await;

    let feil = kilde(&server)
        .hent(&kontekst(Some("testnokkel")))
        .await
        .unwrap_err();
    assert!(matches!(feil, KildeFeil::Http { status: 429 }), "{feil:?}");
}

#[tokio::test]
async fn avvist_nokkel_provas_ikke_igjen() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;

    let feil = kilde(&server)
        .hent(&kontekst(Some("feil")))
        .await
        .unwrap_err();
    assert!(
        matches!(feil, KildeFeil::AvvistNokkel { status: 401 }),
        "{feil:?}"
    );
}

#[tokio::test]
async fn uten_nokkel_sendes_ingenting() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(json(SIDE_1))
        .expect(0)
        .mount(&server)
        .await;

    let feil = kilde(&server).hent(&kontekst(None)).await.unwrap_err();
    assert!(matches!(feil, KildeFeil::ManglerApiNokkel));
}

#[tokio::test]
async fn endret_svarformat_gir_skjemafeil() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(json(r#"{"data": "ikke en liste"}"#))
        .mount(&server)
        .await;

    let feil = kilde(&server)
        .hent(&kontekst(Some("testnokkel")))
        .await
        .unwrap_err();
    assert!(matches!(feil, KildeFeil::Skjemaendring(_)), "{feil:?}");
}

/// Hele kjeden gjennom binæren: `oppdater` henter, matcher mot den innebygde katalogen
/// og lagrer. Alle fixture-radene har EAN-er som finnes i katalogen.
#[tokio::test(flavor = "multi_thread")]
async fn oppdater_lagrer_i_databasen() {
    let server = MockServer::start().await;
    to_sider(&server).await;
    let mappe = TempDir::new().unwrap();
    let url = format!("{}/api/v1", server.uri());

    let databrus = move |mappe: &TempDir, args: &[&str]| {
        let mut kommando = Command::new(env!("CARGO_BIN_EXE_databrus"));
        kommando
            .args(args)
            .env("DATABRUS_KONFIG", mappe.path().join("konfig.toml"))
            .env("DATABRUS_DATA_DIR", mappe.path().join("data"))
            .env("DATABRUS_KASSALAPP_URL", &url)
            .env("KASSALAPP_API_KEY", "testnokkel")
            .env("NO_COLOR", "1");
        kommando
    };

    let (oppdater, ukjente) = tokio::task::spawn_blocking(move || {
        let oppdater = databrus(&mappe, &["oppdater", "--kilde", "kassalapp"]).assert();
        let ukjente = databrus(&mappe, &["produkter", "--ukjente", "--json"]).assert();
        (oppdater, ukjente)
    })
    .await
    .unwrap();

    oppdater.success().stdout(predicate::str::contains(
        "Kassalapp: 14 oppføringer, 14 matchet katalogen",
    ));
    let utdata = ukjente.success().get_output().stdout.clone();
    let json: serde_json::Value = serde_json::from_slice(&utdata).unwrap();
    assert_eq!(json["ukjente"].as_array().unwrap().len(), 0);
}
