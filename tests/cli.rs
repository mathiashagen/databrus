//! Ende-til-ende-tester av binæren. Konfigurasjon og data peker alltid til en midlertidig
//! mappe, slik at testene aldri rører brukerens filer eller nettverket.

use std::process::Command;

use assert_cmd::assert::OutputAssertExt;
use predicates::prelude::*;
use tempfile::TempDir;

fn databrus(mappe: &TempDir) -> Command {
    let mut kommando = Command::new(env!("CARGO_BIN_EXE_databrus"));
    kommando
        .env("DATABRUS_KONFIG", mappe.path().join("konfig.toml"))
        .env("DATABRUS_DATA_DIR", mappe.path().join("data"))
        .env_remove("KASSALAPP_API_KEY")
        .env_remove("DATABRUS_LOGG")
        .env("NO_COLOR", "1");
    kommando
}

fn stdout_json(kommando: &mut Command) -> serde_json::Value {
    let utdata = kommando.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&utdata).expect("gyldig JSON på stdout")
}

#[test]
fn hjelp() {
    let mappe = TempDir::new().unwrap();
    let utdata = databrus(&mappe)
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    insta::assert_snapshot!(String::from_utf8(utdata).unwrap());
}

#[test]
fn frakoblet_uten_data_gir_kode_3() {
    let mappe = TempDir::new().unwrap();
    databrus(&mappe)
        .args(["--frakoblet", "monster", "ultra", "--storrelse", "0,5"])
        .assert()
        .code(3)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("ingen lokale prisdata"));
}

#[test]
fn sok_uten_data_advarer_per_kilde_og_gir_kode_3() {
    let mappe = TempDir::new().unwrap();
    databrus(&mappe)
        .arg("monster")
        .assert()
        .code(3)
        .stderr(predicate::str::contains("Kassalapp: mangler API-nøkkel"))
        .stderr(predicate::str::contains("Oda: ikke implementert"));
}

#[test]
fn oppdater_der_alle_kilder_feiler_gir_kode_1_og_respekterer_gulvet() {
    let mappe = TempDir::new().unwrap();
    databrus(&mappe)
        .arg("oppdater")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("alle kilder feilet"));

    // Et nytt forsøk innen 15 minutter blir ikke sendt.
    databrus(&mappe)
        .arg("oppdater")
        .assert()
        .stderr(predicate::str::contains("neste henting tidligst om"));
}

#[test]
fn butikker_json() {
    let mappe = TempDir::new().unwrap();
    let json = stdout_json(databrus(&mappe).args(["butikker", "--json"]));
    assert_eq!(json["skjemaversjon"], 1);
    let butikker = json["butikker"].as_array().unwrap();
    assert_eq!(butikker.len(), 15);
    assert_eq!(butikker[0]["kjede"], "rema");
    assert!(butikker[0]["sist_hentet"].is_null());
}

#[test]
fn butikker_json_linjer() {
    let mappe = TempDir::new().unwrap();
    let utdata = databrus(&mappe)
        .args(["--json-linjer", "butikker"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let linjer: Vec<serde_json::Value> = String::from_utf8(utdata)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(linjer.len(), 16);
    assert_eq!(linjer[0]["type"], "hode");
    assert_eq!(linjer[0]["skjemaversjon"], 1);
    assert_eq!(linjer[1]["type"], "resultat");
}

#[test]
fn produkter_filtrerer_katalogen() {
    let mappe = TempDir::new().unwrap();
    let json = stdout_json(databrus(&mappe).args(["produkter", "--merke", "monster", "--json"]));
    let produkter = json["produkter"].as_array().unwrap();
    assert!(!produkter.is_empty());
    assert!(produkter.iter().all(|p| p["merke"] == "monster"));
}

#[test]
fn json_og_json_linjer_er_bruksfeil() {
    let mappe = TempDir::new().unwrap();
    databrus(&mappe)
        .args(["--json", "--json-linjer", "butikker"])
        .assert()
        .code(1);
}

#[test]
fn ugyldig_storrelse_er_bruksfeil() {
    let mappe = TempDir::new().unwrap();
    databrus(&mappe)
        .args(["--storrelse", "0,33,0,5"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("ugyldig størrelse"));
}

#[test]
fn konfig_init_vis_og_sett() {
    let mappe = TempDir::new().unwrap();
    databrus(&mappe).args(["konfig", "init"]).assert().success();
    assert!(mappe.path().join("konfig.toml").exists());
    databrus(&mappe).args(["konfig", "init"]).assert().code(1);

    databrus(&mappe)
        .args(["konfig", "sett", "standard_antall", "10"])
        .assert()
        .success();
    databrus(&mappe)
        .args(["konfig", "sett", "henting.min_intervall_minutter", "5"])
        .assert()
        .code(1);

    let json = stdout_json(databrus(&mappe).args(["konfig", "vis", "--json"]));
    assert_eq!(json["konfig"]["standard_antall"], 10);
    assert_eq!(json["konfig"]["henting"]["min_intervall_minutter"], 15);
}

#[test]
fn api_nokkel_vises_aldri() {
    let mappe = TempDir::new().unwrap();
    databrus(&mappe)
        .args([
            "konfig",
            "sett",
            "kilder.kassalapp.api_nokkel",
            "hemmelig123",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("hemmelig123").not());
    databrus(&mappe)
        .args(["konfig", "vis"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hemmelig123").not())
        .stdout(predicate::str::contains("***"));
}

#[test]
fn ugyldig_konfig_stopper_sok_men_ikke_init() {
    let mappe = TempDir::new().unwrap();
    std::fs::write(
        mappe.path().join("konfig.toml"),
        "standard_antall = \"mange\"",
    )
    .unwrap();
    databrus(&mappe)
        .args(["--frakoblet", "monster"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("konfigurasjon"));
    databrus(&mappe)
        .args(["konfig", "init", "--tving"])
        .assert()
        .success();
}

#[test]
fn fullforing_for_powershell() {
    let mappe = TempDir::new().unwrap();
    databrus(&mappe)
        .args(["fullforing", "powershell"])
        .assert()
        .success()
        .stdout(predicate::str::contains("databrus"));
}

#[test]
fn ikke_implementert_gir_kode_1() {
    let mappe = TempDir::new().unwrap();
    databrus(&mappe)
        .args(["overvak", "liste"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("ikke implementert"));
}
