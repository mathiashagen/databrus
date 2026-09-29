//! Databasekontrakten: migreringer og endringsbasert prishistorikk (SPEC §7.2).

use databrus::katalog::Katalog;
use databrus::katalog::matching::{GtinIndeks, match_oppforing};
use databrus::lager::{Endring, Lager, Prisobservasjon};
use databrus::modell::{KildeId, Kjede, Ore, RaaOppforing, Tilbud, Tilbudsinfo};
use jiff::{Timestamp, ToSpan};
use tempfile::TempDir;

fn apne(mappe: &TempDir) -> Lager {
    Lager::apne(&mappe.path().join("undermappe").join("databrus.sqlite")).unwrap()
}

fn oppforing(hyllepris: i64) -> RaaOppforing {
    RaaOppforing {
        kilde: KildeId::Kassalapp,
        kjede: Kjede::Kiwi,
        kilde_produkt_id: "12345".into(),
        gtin: None,
        raanavn: "Monster Energy Ultra White 0,5l".into(),
        raamerke: Some("Monster".into()),
        raa_storrelse: Some("0,5l".into()),
        antall: 1,
        hyllepris: Ore(hyllepris),
        medlemspris: None,
        tilbud: None,
        tilgjengelig: Some(true),
        pant: None,
        kilde_tidspunkt: None,
    }
}

fn tid(timer: i64) -> Timestamp {
    Timestamp::from_second(1_790_000_000).unwrap() + timer.hours()
}

#[test]
fn migrering_oppretter_en_tom_database() {
    let mappe = TempDir::new().unwrap();
    let lager = apne(&mappe);
    assert!(!lager.har_prisdata().unwrap());
    assert_eq!(lager.siste_henting(KildeId::Kassalapp).unwrap(), None);
    // Å åpne en gang til kjører ikke migreringene på nytt.
    drop(lager);
    apne(&mappe);
}

#[test]
fn lik_pris_oppdaterer_bare_sist_sett() {
    let mappe = TempDir::new().unwrap();
    let mut lager = apne(&mappe);
    let raa = oppforing(2490);

    let id = lager.lagre_oppforing(&raa, None, tid(0)).unwrap();
    let forste = lager
        .registrer_pris(id, &Prisobservasjon::fra(&raa), tid(0))
        .unwrap();
    assert!(matches!(forste, Endring::NyttIntervall(_)));

    // Samme oppføring gir samme id, og samme pris gir ingen ny rad.
    let id_igjen = lager.lagre_oppforing(&raa, None, tid(6)).unwrap();
    assert_eq!(id, id_igjen);
    let andre = lager
        .registrer_pris(id, &Prisobservasjon::fra(&raa), tid(6))
        .unwrap();
    assert_eq!(andre, Endring::Uendret);
    assert_eq!(lager.antall_prisintervaller().unwrap(), 1);
    assert!(lager.har_prisdata().unwrap());
}

#[test]
fn sist_sett_gar_aldri_bakover() {
    let mappe = TempDir::new().unwrap();
    let mut lager = apne(&mappe);
    let raa = oppforing(2490);
    let id = lager.lagre_oppforing(&raa, None, tid(0)).unwrap();
    lager
        .registrer_pris(id, &Prisobservasjon::fra(&raa), tid(24))
        .unwrap();
    assert_eq!(
        lager.nyeste_pris_per_kjede().unwrap().get(&Kjede::Kiwi),
        Some(&tid(24))
    );
    // Samme pris, men kilden oppgir et eldre tidspunkt.
    let endring = lager
        .registrer_pris(id, &Prisobservasjon::fra(&raa), tid(2))
        .unwrap();
    assert_eq!(endring, Endring::Uendret);
    assert_eq!(lager.siste_pris_sett(id).unwrap(), Some(tid(24)));
}

#[test]
fn ny_pris_eller_nytt_tilbud_gir_nytt_intervall() {
    let mappe = TempDir::new().unwrap();
    let mut lager = apne(&mappe);
    let raa = oppforing(2490);
    let id = lager.lagre_oppforing(&raa, None, tid(0)).unwrap();
    lager
        .registrer_pris(id, &Prisobservasjon::fra(&raa), tid(0))
        .unwrap();

    let billigere = oppforing(2290);
    let endring = lager
        .registrer_pris(id, &Prisobservasjon::fra(&billigere), tid(6))
        .unwrap();
    assert!(matches!(endring, Endring::NyttIntervall(_)));

    let mut med_tilbud = oppforing(2290);
    med_tilbud.tilbud = Some(Tilbudsinfo {
        tilbud: Tilbud::NForM { n: 3, m: 2 },
        gyldig_fra: None,
        gyldig_til: None,
        kilde_merket: true,
    });
    let endring = lager
        .registrer_pris(id, &Prisobservasjon::fra(&med_tilbud), tid(12))
        .unwrap();
    assert!(matches!(endring, Endring::NyttIntervall(_)));

    // Tilbake til en tidligere pris er også en endring.
    let endring = lager
        .registrer_pris(id, &Prisobservasjon::fra(&raa), tid(18))
        .unwrap();
    assert!(matches!(endring, Endring::NyttIntervall(_)));
    assert_eq!(lager.antall_prisintervaller().unwrap(), 4);
}

#[test]
fn matchet_oppforing_peker_pa_katalogproduktet() {
    let mappe = TempDir::new().unwrap();
    let mut lager = apne(&mappe);
    let katalog = Katalog::fra_toml(
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
    lager.synk_katalog(&katalog).unwrap();

    let mut pakke = oppforing(6990);
    pakke.kilde_produkt_id = "pakke-1".into();
    pakke.gtin = Some("7040110569915".into());
    let treff = match_oppforing(&GtinIndeks::bygg(&katalog), &pakke).unwrap();
    assert_eq!(treff.antall, 4);
    lager.lagre_oppforing(&pakke, Some(&treff), tid(0)).unwrap();

    // Umatchede oppføringer listes for `produkter --ukjente`.
    lager
        .lagre_oppforing(&oppforing(2490), None, tid(0))
        .unwrap();
    let ukjente = lager.umatchede_oppforinger().unwrap();
    assert_eq!(ukjente.len(), 1);
    assert_eq!(ukjente[0].kilde_produkt_id, "12345");
}

/// Engrossnett selger brett under enkeltboksens EAN, og andre butikker kan bruke en
/// pakke-EAN for én boks. Pakningen skal komme fra oppføringens navn.
#[test]
fn pakning_kommer_fra_navnet_ikke_ean() {
    let mappe = TempDir::new().unwrap();
    let mut lager = apne(&mappe);
    let katalog = Katalog::fra_toml(
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
    lager.synk_katalog(&katalog).unwrap();
    let indeks = GtinIndeks::bygg(&katalog);

    // Brett under enkeltboksens EAN.
    let mut brett = oppforing(73_531);
    brett.kjede = Kjede::Engrossnett;
    brett.kilde_produkt_id = "brett".into();
    brett.raanavn = "Monster Energy 24x500ml".into();
    brett.antall = 24;
    brett.gtin = Some("5060166693732".into());
    // Én boks under pakke-EAN-en.
    let mut enkel = oppforing(3090);
    enkel.kjede = Kjede::Oda;
    enkel.kilde_produkt_id = "enkel".into();
    enkel.raanavn = "Monster Energy 0,5 l".into();
    enkel.gtin = Some("5060517881412".into());

    for raa in [&brett, &enkel] {
        let treff = match_oppforing(&indeks, raa).unwrap();
        let id = lager.lagre_oppforing(raa, Some(&treff), tid(0)).unwrap();
        lager
            .registrer_pris(id, &Prisobservasjon::fra(raa), tid(0))
            .unwrap();
    }
    let mut antall: Vec<(Kjede, u32)> = lager
        .siste_priser()
        .unwrap()
        .into_iter()
        .map(|p| (p.kjede, p.antall))
        .collect();
    antall.sort();
    assert_eq!(antall, [(Kjede::Oda, 1), (Kjede::Engrossnett, 24)]);
}

#[test]
fn hentelogg() {
    let mappe = TempDir::new().unwrap();
    let lager = apne(&mappe);
    lager
        .logg_henting(KildeId::Oda, tid(0), tid(0), Ok(42))
        .unwrap();
    lager
        .logg_henting(KildeId::Oda, tid(1), tid(1), Err("HTTP 503"))
        .unwrap();

    let siste = lager.siste_henting(KildeId::Oda).unwrap().unwrap();
    assert!(!siste.ok);
    assert_eq!(siste.feilmelding.as_deref(), Some("HTTP 503"));
    assert_eq!(
        lager.siste_vellykkede_henting(KildeId::Oda).unwrap(),
        Some(tid(0))
    );
    assert_eq!(lager.siste_henting(KildeId::Coop).unwrap(), None);
}
