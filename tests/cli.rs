//! End-to-end tests of the binary. Config and data always point to a temporary directory,
//! so the tests never touch the user's files or the network.
//!
//! The command lines and the expected output are Norwegian, since that is the user-facing
//! language.

use std::process::Command;

use assert_cmd::assert::OutputAssertExt;
use predicates::prelude::*;
use tempfile::TempDir;

fn databrus(dir: &TempDir) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_databrus"));
    command
        .env("DATABRUS_KONFIG", dir.path().join("konfig.toml"))
        .env("DATABRUS_DATA_DIR", dir.path().join("data"))
        .env_remove("KASSALAPP_API_KEY")
        // Oda needs no key, so point it at an invalid URL: it fails at once, offline.
        .env("DATABRUS_ODA_URL", "ikke-en-url")
        .env_remove("DATABRUS_LOGG")
        .env("NO_COLOR", "1");
    command
}

fn stdout_json(command: &mut Command) -> serde_json::Value {
    let output = command.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&output).expect("valid JSON on stdout")
}

#[test]
fn help() {
    let dir = TempDir::new().unwrap();
    let output = databrus(&dir)
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    insta::assert_snapshot!(String::from_utf8(output).unwrap());
}

#[test]
fn offline_without_data_exits_with_3() {
    let dir = TempDir::new().unwrap();
    databrus(&dir)
        .args(["--frakoblet", "monster", "ultra", "--storrelse", "0,5"])
        .assert()
        .code(3)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("ingen lokale prisdata"));
}

#[test]
fn search_without_data_warns_per_source_and_exits_with_3() {
    let dir = TempDir::new().unwrap();
    databrus(&dir)
        .arg("monster")
        .assert()
        .code(3)
        .stderr(predicate::str::contains("Kassalapp: mangler API-nøkkel"))
        .stderr(predicate::str::contains("Oda: ugyldig URL"))
        // Rema and Coop are off by default and stay quiet.
        .stderr(predicate::str::contains("Rema").not())
        .stderr(predicate::str::contains("Coop").not());
}

#[test]
fn update_where_all_sources_fail_exits_with_1_and_respects_the_floor() {
    let dir = TempDir::new().unwrap();
    databrus(&dir)
        .arg("oppdater")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("alle kilder feilet"));

    // A new attempt within 15 minutes is not sent.
    databrus(&dir)
        .arg("oppdater")
        .assert()
        .stderr(predicate::str::contains("neste henting tidligst om"));
}

#[test]
fn stores_json() {
    let dir = TempDir::new().unwrap();
    let json = stdout_json(databrus(&dir).args(["butikker", "--json"]));
    assert_eq!(json["skjemaversjon"], 1);
    let stores = json["butikker"].as_array().unwrap();
    assert_eq!(stores.len(), 15);
    assert_eq!(stores[0]["kjede"], "rema");
    assert_eq!(stores[0]["navn"], "Rema 1000");
    assert!(stores[0]["sist_hentet"].is_null());
    assert!(stores[0].get("nyeste_pris").is_some());
}

#[test]
fn stores_json_lines() {
    let dir = TempDir::new().unwrap();
    let output = databrus(&dir)
        .args(["--json-linjer", "butikker"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let lines: Vec<serde_json::Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 16);
    assert_eq!(lines[0]["type"], "hode");
    assert_eq!(lines[0]["skjemaversjon"], 1);
    assert_eq!(lines[1]["type"], "resultat");
}

#[test]
fn products_filters_the_catalog() {
    let dir = TempDir::new().unwrap();
    let json = stdout_json(databrus(&dir).args(["produkter", "--merke", "monster", "--json"]));
    let products = json["produkter"].as_array().unwrap();
    assert!(!products.is_empty());
    assert!(products.iter().all(|p| p["merke"] == "monster"));
    // The catalog keys stay Norwegian in JSON.
    assert!(products[0].get("volum_ml").is_some());
    assert!(products[0].get("sukkerfri").is_some());
}

#[test]
fn json_and_json_lines_is_a_usage_error() {
    let dir = TempDir::new().unwrap();
    databrus(&dir)
        .args(["--json", "--json-linjer", "butikker"])
        .assert()
        .code(1);
}

#[test]
fn invalid_size_is_a_usage_error() {
    let dir = TempDir::new().unwrap();
    databrus(&dir)
        .args(["--storrelse", "0,33,0,5"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("ugyldig størrelse"));
}

#[test]
fn config_init_show_and_set() {
    let dir = TempDir::new().unwrap();
    databrus(&dir).args(["konfig", "init"]).assert().success();
    assert!(dir.path().join("konfig.toml").exists());
    databrus(&dir).args(["konfig", "init"]).assert().code(1);

    databrus(&dir)
        .args(["konfig", "sett", "standard_antall", "10"])
        .assert()
        .success();
    databrus(&dir)
        .args(["konfig", "sett", "henting.min_intervall_minutter", "5"])
        .assert()
        .code(1);

    let json = stdout_json(databrus(&dir).args(["konfig", "vis", "--json"]));
    assert_eq!(json["konfig"]["standard_antall"], 10);
    assert_eq!(json["konfig"]["henting"]["min_intervall_minutter"], 15);
    assert_eq!(json["konfig"]["kilder"]["kassalapp"]["aktiv"], true);
}

#[test]
fn api_key_is_never_shown() {
    let dir = TempDir::new().unwrap();
    databrus(&dir)
        .args([
            "konfig",
            "sett",
            "kilder.kassalapp.api_nokkel",
            "hemmelig123",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("hemmelig123").not());
    databrus(&dir)
        .args(["konfig", "vis"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hemmelig123").not())
        .stdout(predicate::str::contains("***"));
}

#[test]
fn invalid_config_stops_search_but_not_init() {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("konfig.toml"),
        "standard_antall = \"mange\"",
    )
    .unwrap();
    databrus(&dir)
        .args(["--frakoblet", "monster"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("konfigurasjon"));
    databrus(&dir)
        .args(["konfig", "init", "--tving"])
        .assert()
        .success();
}

#[test]
fn completions_for_powershell() {
    let dir = TempDir::new().unwrap();
    databrus(&dir)
        .args(["fullforing", "powershell"])
        .assert()
        .success()
        .stdout(predicate::str::contains("databrus"));
}

#[test]
fn alerts_list_and_remove_without_alerts() {
    let dir = TempDir::new().unwrap();
    databrus(&dir)
        .args(["overvak", "liste"])
        .assert()
        .success()
        .stderr(predicate::str::contains("ingen prisvarsler"));
    databrus(&dir)
        .args(["overvak", "fjern", "7"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("fant ikke varsel 7"));
}
