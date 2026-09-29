//! Delte domenetyper.
//!
//! Penger er alltid heltall øre ([`Ore`]) og volum alltid heltall milliliter ([`Ml`]);
//! flyttall brukes bare til visning (SPEC §5).

use std::fmt;
use std::num::NonZeroU32;

use clap::ValueEnum;
use jiff::Timestamp;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};

/// Nåtid avrundet ned til hele sekunder – det er oppløsningen i databasen og i JSON.
pub fn na() -> Timestamp {
    let na = Timestamp::now();
    Timestamp::from_second(na.as_second()).unwrap_or(na)
}

/// Et beløp i øre.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Ore(pub i64);

impl Ore {
    pub const fn fra_kr(kr: i64) -> Self {
        Self(kr * 100)
    }
}

/// Et volum i milliliter. Aldri null.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Ml(NonZeroU32);

impl Ml {
    pub const fn new(ml: u32) -> Option<Self> {
        match NonZeroU32::new(ml) {
            Some(ml) => Some(Self(ml)),
            None => None,
        }
    }

    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

/// Identifikator for et kanonisk produkt, f.eks. `monster-ultra-white-500-boks`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProduktId(pub String);

impl fmt::Display for ProduktId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// En priskilde (SPEC §4.2).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, ValueEnum, Serialize, Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum KildeId {
    Kassalapp,
    Oda,
    Rema,
    Coop,
}

impl KildeId {
    pub const ALLE: [KildeId; 4] = [
        KildeId::Kassalapp,
        KildeId::Oda,
        KildeId::Rema,
        KildeId::Coop,
    ];

    pub const fn slug(self) -> &'static str {
        match self {
            KildeId::Kassalapp => "kassalapp",
            KildeId::Oda => "oda",
            KildeId::Rema => "rema",
            KildeId::Coop => "coop",
        }
    }

    pub const fn visningsnavn(self) -> &'static str {
        match self {
            KildeId::Kassalapp => "Kassalapp",
            KildeId::Oda => "Oda",
            KildeId::Rema => "Rema 1000-tilbud",
            KildeId::Coop => "Coop-tilbud",
        }
    }

    pub fn fra_slug(slug: &str) -> Option<Self> {
        Self::ALLE.into_iter().find(|k| k.slug() == slug)
    }
}

impl fmt::Display for KildeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.slug())
    }
}

/// En dagligvarekjede (SPEC §4.4). Prisene er på kjedenivå.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, ValueEnum, Serialize, Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum Kjede {
    Rema,
    Kiwi,
    Meny,
    Spar,
    Joker,
    CoopExtra,
    CoopObs,
    CoopMega,
    CoopPrix,
    Bunnpris,
    Oda,
    Europris,
    Engrossnett,
    Havaristen,
    Fastcandy,
}

impl Kjede {
    pub const ALLE: [Kjede; 15] = [
        Kjede::Rema,
        Kjede::Kiwi,
        Kjede::Meny,
        Kjede::Spar,
        Kjede::Joker,
        Kjede::CoopExtra,
        Kjede::CoopObs,
        Kjede::CoopMega,
        Kjede::CoopPrix,
        Kjede::Bunnpris,
        Kjede::Oda,
        Kjede::Europris,
        Kjede::Engrossnett,
        Kjede::Havaristen,
        Kjede::Fastcandy,
    ];

    pub const COOP: [Kjede; 4] = [
        Kjede::CoopExtra,
        Kjede::CoopObs,
        Kjede::CoopMega,
        Kjede::CoopPrix,
    ];

    pub const fn slug(self) -> &'static str {
        match self {
            Kjede::Rema => "rema",
            Kjede::Kiwi => "kiwi",
            Kjede::Meny => "meny",
            Kjede::Spar => "spar",
            Kjede::Joker => "joker",
            Kjede::CoopExtra => "coop-extra",
            Kjede::CoopObs => "coop-obs",
            Kjede::CoopMega => "coop-mega",
            Kjede::CoopPrix => "coop-prix",
            Kjede::Bunnpris => "bunnpris",
            Kjede::Oda => "oda",
            Kjede::Europris => "europris",
            Kjede::Engrossnett => "engrossnett",
            Kjede::Havaristen => "havaristen",
            Kjede::Fastcandy => "fastcandy",
        }
    }

    pub const fn visningsnavn(self) -> &'static str {
        match self {
            Kjede::Rema => "Rema 1000",
            Kjede::Kiwi => "Kiwi",
            Kjede::Meny => "Meny",
            Kjede::Spar => "Spar",
            Kjede::Joker => "Joker",
            Kjede::CoopExtra => "Coop Extra",
            Kjede::CoopObs => "Coop Obs",
            Kjede::CoopMega => "Coop Mega",
            Kjede::CoopPrix => "Coop Prix",
            Kjede::Bunnpris => "Bunnpris",
            Kjede::Oda => "Oda",
            Kjede::Europris => "Europris",
            Kjede::Engrossnett => "Engrossnett",
            Kjede::Havaristen => "Havaristen",
            Kjede::Fastcandy => "Fastcandy",
        }
    }

    pub const fn gruppe(self) -> Option<&'static str> {
        match self {
            Kjede::Rema => Some("Reitan"),
            Kjede::Kiwi | Kjede::Meny | Kjede::Spar | Kjede::Joker => Some("NorgesGruppen"),
            Kjede::CoopExtra | Kjede::CoopObs | Kjede::CoopMega | Kjede::CoopPrix => Some("Coop"),
            Kjede::Bunnpris
            | Kjede::Oda
            | Kjede::Europris
            | Kjede::Engrossnett
            | Kjede::Havaristen
            | Kjede::Fastcandy => None,
        }
    }

    pub fn fra_slug(slug: &str) -> Option<Self> {
        Self::ALLE.into_iter().find(|k| k.slug() == slug)
    }
}

impl fmt::Display for Kjede {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.visningsnavn())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Beholder {
    Boks,
    Flaske,
}

impl Beholder {
    pub const fn slug(self) -> &'static str {
        match self {
            Beholder::Boks => "boks",
            Beholder::Flaske => "flaske",
        }
    }
}

/// Lojalitetsprogrammer med egne medlemspriser (SPEC §5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Medlemsprogram {
    Coop,
    Trumf,
    Ae,
    KiwiPluss,
}

impl Medlemsprogram {
    pub const fn slug(self) -> &'static str {
        match self {
            Medlemsprogram::Coop => "coop",
            Medlemsprogram::Trumf => "trumf",
            Medlemsprogram::Ae => "ae",
            Medlemsprogram::KiwiPluss => "kiwi-pluss",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Medlemspris {
    pub pris: Ore,
    pub program: Medlemsprogram,
}

/// Tilbudstyper (SPEC §5.2). Flerpakninger er ikke et tilbud, men en egen
/// oppføring med `antall > 1`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Tilbud {
    /// «Nå 15,90»
    Fastpris { pris_ore: Ore },
    /// «3 for 2»
    NForM { n: u32, m: u32 },
    /// «2 for 50 kr»
    NForSum { n: u32, sum_ore: Ore },
    /// «30 % rabatt»
    Prosent { prosent: u32 },
    /// «3. stk gratis» (n = 3, rabatt 100) eller «2. til halv pris» (n = 2, rabatt 50)
    NteVare { n: u32, rabatt_prosent: u32 },
}

/// Et tilbud med gyldighet, slik det lagres og vises.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tilbudsinfo {
    #[serde(flatten)]
    pub tilbud: Tilbud,
    pub gyldig_fra: Option<Date>,
    pub gyldig_til: Option<Date>,
    /// Kilden selv merker dette som et tilbud (til forskjell fra et utledet prisfall).
    pub kilde_merket: bool,
}

/// Et kanonisk produkt fra katalogen (SPEC §6.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Produkt {
    pub id: ProduktId,
    pub navn: String,
    pub merke: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linje: Option<String>,
    pub smak: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub smak_alias: Vec<String>,
    pub sukkerfri: bool,
    pub volum_ml: Ml,
    pub beholder: Beholder,
    #[serde(default)]
    pub gtin: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub koffein_mg_per_100ml: Option<u16>,
    #[serde(default)]
    pub egenmerke: bool,
}

/// En rå oppføring slik en kilde leverer den, før matching mot katalogen (SPEC §4.1).
#[derive(Debug, Clone, PartialEq)]
pub struct RaaOppforing {
    pub kilde: KildeId,
    pub kjede: Kjede,
    pub kilde_produkt_id: String,
    pub gtin: Option<String>,
    pub raanavn: String,
    /// Merket slik kilden skriver det (ofte inkonsekvent: «Red bull», «RED BULL»).
    pub raamerke: Option<String>,
    pub raa_storrelse: Option<String>,
    /// Antall beholdere i den salgbare enheten (1 for enkeltbokser).
    pub antall: u32,
    /// Ordinær pris for den salgbare enheten.
    pub hyllepris: Ore,
    pub medlemspris: Option<Medlemspris>,
    pub tilbud: Option<Tilbudsinfo>,
    pub tilgjengelig: Option<bool>,
    /// Pant slik kilden oppgir den, hvis den gjør det (SPEC §5.4).
    pub pant: Option<Ore>,
    /// Når kilden sist så denne prisen. Dette er observasjonstiden i historikken – ikke
    /// når vi hentet – slik at en gammel pris aldri ser fersk ut (SPEC §4.5).
    pub kilde_tidspunkt: Option<Timestamp>,
}

/// Rangeringsverdict (SPEC §7.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Vurdering {
    Supert,
    Bra,
    Middels,
    Lureri,
    Ukjent,
}

/// Merke i Tilbud-kolonnen (SPEC §7.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Tilbudsmerke {
    /// Kilden merker et aktivt tilbud.
    Kampanje,
    /// Ingen merket tilbud, men prisen ligger klart under 90-dagersmedianen.
    Prisfall,
}

/// Hvilken pris rangeringen brukte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BruktPris {
    Hyllepris,
    Medlemspris,
    Tilbud,
}

/// Produktdelen av et søkeresultat (SPEC §9.1).
#[derive(Debug, Clone, Serialize)]
pub struct Produktsammendrag {
    pub id: ProduktId,
    pub navn: String,
    pub merke: String,
    pub smak: String,
    pub sukkerfri: bool,
    pub volum_ml: Ml,
    pub beholder: Beholder,
    pub egenmerke: bool,
    pub verifisert: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Vurderingsinfo {
    pub verdi: Vurdering,
    pub l30_ore: Option<Ore>,
    pub m90_ore: Option<Ore>,
    pub atl_ore: Option<Ore>,
    pub dekning_dager: u32,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Prisspenn {
    pub min_ore: Ore,
    pub maks_ore: Ore,
}

/// Én rangert rad: et produkt hos en kjede (SPEC §9.1). Dette er JSON-kontrakten.
#[derive(Debug, Clone, Serialize)]
pub struct Resultat {
    pub produkt: Produktsammendrag,
    pub kjede: Kjede,
    pub kilde: KildeId,
    pub antall_i_pakke: u32,
    pub hyllepris_ore: Ore,
    pub medlemspris_ore: Option<Ore>,
    pub medlemsprogram: Option<Medlemsprogram>,
    pub effektiv_enhetspris_ore: Ore,
    pub literpris_ore: Ore,
    pub minsteantall: u32,
    pub brukt_pris: BruktPris,
    pub pant_ore: Ore,
    pub pant_minsteantall_ore: Ore,
    pub tilbud: Option<Tilbudsinfo>,
    pub tilbudsmerke: Option<Tilbudsmerke>,
    pub vurdering: Vurderingsinfo,
    pub prisspenn: Option<Prisspenn>,
    pub tilgjengelig: bool,
    pub sist_sett: Timestamp,
    pub alder_timer: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kjedeslug_gar_tur_retur() {
        for kjede in Kjede::ALLE {
            assert_eq!(Kjede::fra_slug(kjede.slug()), Some(kjede));
            // Slug, clap-navn og serde-navn må være det samme.
            let clap_navn = kjede.to_possible_value().map(|v| v.get_name().to_owned());
            assert_eq!(clap_navn.as_deref(), Some(kjede.slug()));
            let json = serde_json::to_string(&kjede).unwrap();
            assert_eq!(json, format!("\"{}\"", kjede.slug()));
        }
    }

    #[test]
    fn kildeslug_gar_tur_retur() {
        for kilde in KildeId::ALLE {
            assert_eq!(KildeId::fra_slug(kilde.slug()), Some(kilde));
        }
    }

    #[test]
    fn tilbud_serialiseres_som_i_spec() {
        let info = Tilbudsinfo {
            tilbud: Tilbud::NForSum {
                n: 3,
                sum_ore: Ore(4770),
            },
            gyldig_fra: Some(jiff::civil::date(2026, 9, 28)),
            gyldig_til: Some(jiff::civil::date(2026, 10, 4)),
            kilde_merket: true,
        };
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "n_for_sum", "n": 3, "sum_ore": 4770,
                "gyldig_fra": "2026-09-28", "gyldig_til": "2026-10-04", "kilde_merket": true
            })
        );
        let tilbake: Tilbudsinfo = serde_json::from_value(json).unwrap();
        assert_eq!(tilbake, info);
    }

    #[test]
    fn ml_kan_ikke_vaere_null() {
        assert!(Ml::new(0).is_none());
        assert_eq!(Ml::new(500).map(Ml::get), Some(500));
    }
}
