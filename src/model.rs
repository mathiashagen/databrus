//! Shared domain types.
//!
//! Money is always integer øre ([`Ore`]) and volume always integer millilitres ([`Ml`]);
//! floats are only used for display (SPEC §5).
//!
//! Types that appear in JSON output or in the catalog file keep their Norwegian keys via
//! `serde(rename)`: the Rust names are English, the user-facing contract is Norwegian.

use std::fmt;
use std::num::NonZeroU32;

use clap::ValueEnum;
use jiff::Timestamp;
use jiff::civil::Date;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The current time truncated to whole seconds, the resolution used in the database and
/// in JSON.
pub fn now() -> Timestamp {
    let now = Timestamp::now();
    Timestamp::from_second(now.as_second()).unwrap_or(now)
}

/// An amount in øre (1/100 NOK).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(transparent)]
pub struct Ore(pub i64);

impl Ore {
    pub const fn from_kr(kr: i64) -> Self {
        Self(kr * 100)
    }
}

/// A volume in millilitres. Never zero.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
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

/// Identifier of a canonical product, e.g. `monster-ultra-white-500-boks`.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct ProductId(pub String);

impl fmt::Display for ProductId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A price source (SPEC §4.2).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    ValueEnum,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum SourceId {
    Kassalapp,
    Oda,
    Rema,
    Coop,
}

impl SourceId {
    pub const ALL: [SourceId; 4] = [
        SourceId::Kassalapp,
        SourceId::Oda,
        SourceId::Rema,
        SourceId::Coop,
    ];

    pub const fn slug(self) -> &'static str {
        match self {
            SourceId::Kassalapp => "kassalapp",
            SourceId::Oda => "oda",
            SourceId::Rema => "rema",
            SourceId::Coop => "coop",
        }
    }

    /// Whether the source is the chain's own (Oda) rather than the Kassalapp aggregator.
    /// A direct source wins ties and its offers win conflicts (SPEC §4.3).
    pub const fn is_direct(self) -> bool {
        !matches!(self, SourceId::Kassalapp)
    }

    /// Norwegian name shown to the user.
    pub const fn display_name(self) -> &'static str {
        match self {
            SourceId::Kassalapp => "Kassalapp",
            SourceId::Oda => "Oda",
            SourceId::Rema => "Rema 1000-tilbud",
            SourceId::Coop => "Coop-tilbud",
        }
    }

    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.slug() == slug)
    }
}

impl fmt::Display for SourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.slug())
    }
}

/// A grocery chain (SPEC §4.4). Prices are chain-level.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    ValueEnum,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum Chain {
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

impl Chain {
    pub const ALL: [Chain; 15] = [
        Chain::Rema,
        Chain::Kiwi,
        Chain::Meny,
        Chain::Spar,
        Chain::Joker,
        Chain::CoopExtra,
        Chain::CoopObs,
        Chain::CoopMega,
        Chain::CoopPrix,
        Chain::Bunnpris,
        Chain::Oda,
        Chain::Europris,
        Chain::Engrossnett,
        Chain::Havaristen,
        Chain::Fastcandy,
    ];

    pub const COOP: [Chain; 4] = [
        Chain::CoopExtra,
        Chain::CoopObs,
        Chain::CoopMega,
        Chain::CoopPrix,
    ];

    pub const fn slug(self) -> &'static str {
        match self {
            Chain::Rema => "rema",
            Chain::Kiwi => "kiwi",
            Chain::Meny => "meny",
            Chain::Spar => "spar",
            Chain::Joker => "joker",
            Chain::CoopExtra => "coop-extra",
            Chain::CoopObs => "coop-obs",
            Chain::CoopMega => "coop-mega",
            Chain::CoopPrix => "coop-prix",
            Chain::Bunnpris => "bunnpris",
            Chain::Oda => "oda",
            Chain::Europris => "europris",
            Chain::Engrossnett => "engrossnett",
            Chain::Havaristen => "havaristen",
            Chain::Fastcandy => "fastcandy",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Chain::Rema => "Rema 1000",
            Chain::Kiwi => "Kiwi",
            Chain::Meny => "Meny",
            Chain::Spar => "Spar",
            Chain::Joker => "Joker",
            Chain::CoopExtra => "Coop Extra",
            Chain::CoopObs => "Coop Obs",
            Chain::CoopMega => "Coop Mega",
            Chain::CoopPrix => "Coop Prix",
            Chain::Bunnpris => "Bunnpris",
            Chain::Oda => "Oda",
            Chain::Europris => "Europris",
            Chain::Engrossnett => "Engrossnett",
            Chain::Havaristen => "Havaristen",
            Chain::Fastcandy => "Fastcandy",
        }
    }

    pub const fn group(self) -> Option<&'static str> {
        match self {
            Chain::Rema => Some("Reitan"),
            Chain::Kiwi | Chain::Meny | Chain::Spar | Chain::Joker => Some("NorgesGruppen"),
            Chain::CoopExtra | Chain::CoopObs | Chain::CoopMega | Chain::CoopPrix => Some("Coop"),
            Chain::Bunnpris
            | Chain::Oda
            | Chain::Europris
            | Chain::Engrossnett
            | Chain::Havaristen
            | Chain::Fastcandy => None,
        }
    }

    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.slug() == slug)
    }
}

impl fmt::Display for Chain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_name())
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, ValueEnum, Serialize, Deserialize, JsonSchema,
)]
pub enum Container {
    #[value(name = "boks")]
    #[serde(rename = "boks")]
    Can,
    #[value(name = "flaske")]
    #[serde(rename = "flaske")]
    Bottle,
}

impl Container {
    pub const fn slug(self) -> &'static str {
        match self {
            Container::Can => "boks",
            Container::Bottle => "flaske",
        }
    }
}

/// Loyalty programmes with their own member prices (SPEC §5.3).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, ValueEnum, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum MembershipProgram {
    Coop,
    Trumf,
    Ae,
    KiwiPluss,
}

impl MembershipProgram {
    pub const ALL: [MembershipProgram; 4] = [
        MembershipProgram::Coop,
        MembershipProgram::Trumf,
        MembershipProgram::Ae,
        MembershipProgram::KiwiPluss,
    ];

    pub const fn slug(self) -> &'static str {
        match self {
            MembershipProgram::Coop => "coop",
            MembershipProgram::Trumf => "trumf",
            MembershipProgram::Ae => "ae",
            MembershipProgram::KiwiPluss => "kiwi-pluss",
        }
    }

    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.slug() == slug)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberPrice {
    #[serde(rename = "pris")]
    pub price: Ore,
    pub program: MembershipProgram,
}

/// Offer types (SPEC §5.2). Multipacks are not offers but separate listings with a
/// pack size above 1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum Offer {
    /// "Nå 15,90"
    #[serde(rename = "fastpris")]
    FixedPrice {
        #[serde(rename = "pris_ore")]
        price: Ore,
    },
    /// "3 for 2"
    #[serde(rename = "n_for_m")]
    NForM { n: u32, m: u32 },
    /// "2 for 50 kr"
    #[serde(rename = "n_for_sum")]
    NForSum {
        n: u32,
        #[serde(rename = "sum_ore")]
        sum: Ore,
    },
    /// "30 % rabatt"
    #[serde(rename = "prosent")]
    Percent {
        #[serde(rename = "prosent")]
        percent: u32,
    },
    /// "3. stk gratis" (n = 3, discount 100) or "2. til halv pris" (n = 2, discount 50)
    #[serde(rename = "nte_vare")]
    NthItem {
        n: u32,
        #[serde(rename = "rabatt_prosent")]
        discount_percent: u32,
    },
}

/// An offer with its validity, as stored and shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OfferInfo {
    #[serde(flatten)]
    pub offer: Offer,
    #[serde(rename = "gyldig_fra")]
    pub valid_from: Option<Date>,
    #[serde(rename = "gyldig_til")]
    pub valid_to: Option<Date>,
    /// The source itself flags this as an offer (as opposed to a derived price drop).
    #[serde(rename = "kilde_merket")]
    pub source_flagged: bool,
}

/// A canonical product from the catalog (SPEC §6.1). The keys are those of the
/// user-editable `katalog.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Product {
    pub id: ProductId,
    #[serde(rename = "navn")]
    pub name: String,
    #[serde(rename = "merke")]
    pub brand: String,
    #[serde(rename = "linje", default, skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    #[serde(rename = "smak")]
    pub flavor: String,
    #[serde(rename = "smak_alias", default, skip_serializing_if = "Vec::is_empty")]
    pub flavor_aliases: Vec<String>,
    #[serde(rename = "sukkerfri")]
    pub sugar_free: bool,
    #[serde(rename = "volum_ml")]
    pub volume: Ml,
    #[serde(rename = "beholder")]
    pub container: Container,
    #[serde(default)]
    pub gtin: Vec<String>,
    #[serde(
        rename = "koffein_mg_per_100ml",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub caffeine_mg_per_100ml: Option<u16>,
    #[serde(rename = "egenmerke", default)]
    pub store_brand: bool,
}

/// A raw listing as a source delivers it, before matching against the catalog
/// (SPEC §4.1).
#[derive(Debug, Clone, PartialEq)]
pub struct RawListing {
    pub source: SourceId,
    pub chain: Chain,
    pub source_product_id: String,
    pub gtin: Option<String>,
    pub raw_name: String,
    /// The brand as the source spells it (often inconsistent: "Red bull", "RED BULL").
    pub raw_brand: Option<String>,
    pub raw_size: Option<String>,
    /// Containers in the sellable unit (1 for single cans).
    pub pack_size: u32,
    /// Ordinary price of the sellable unit.
    pub shelf_price: Ore,
    pub member_price: Option<MemberPrice>,
    pub offer: Option<OfferInfo>,
    pub available: Option<bool>,
    /// Deposit as reported by the source, if it reports one (SPEC §5.4).
    pub deposit: Option<Ore>,
    /// When the source last saw this price. This is the observation time in the history,
    /// not when we fetched it, so an old price never looks fresh (SPEC §4.5).
    pub source_timestamp: Option<Timestamp>,
}

/// Ranking verdict (SPEC §7.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, JsonSchema)]
pub enum Verdict {
    #[serde(rename = "SUPERT")]
    Great,
    #[serde(rename = "BRA")]
    Good,
    #[serde(rename = "MIDDELS")]
    Fair,
    #[serde(rename = "LURERI")]
    Fake,
    #[serde(rename = "UKJENT")]
    Unknown,
}

impl Verdict {
    /// The Norwegian name, as in JSON.
    pub const fn label(self) -> &'static str {
        match self {
            Verdict::Great => "SUPERT",
            Verdict::Good => "BRA",
            Verdict::Fair => "MIDDELS",
            Verdict::Fake => "LURERI",
            Verdict::Unknown => "UKJENT",
        }
    }
}

/// Badge in the Tilbud column (SPEC §7.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
pub enum DealBadge {
    /// The source flags an active offer.
    #[serde(rename = "KAMPANJE")]
    Campaign,
    /// No flagged offer, but the price is clearly below the 90-day median.
    #[serde(rename = "PRISFALL")]
    PriceDrop,
}

/// Which price the ranking used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
pub enum PriceBasis {
    #[serde(rename = "hyllepris")]
    Shelf,
    #[serde(rename = "medlemspris")]
    Member,
    #[serde(rename = "tilbud")]
    Offer,
}

/// The product part of a search result (SPEC §9.1).
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ProductSummary {
    pub id: ProductId,
    #[serde(rename = "navn")]
    pub name: String,
    #[serde(rename = "merke")]
    pub brand: String,
    #[serde(rename = "smak")]
    pub flavor: String,
    #[serde(rename = "sukkerfri")]
    pub sugar_free: bool,
    #[serde(rename = "volum_ml")]
    pub volume: Ml,
    #[serde(rename = "beholder")]
    pub container: Container,
    #[serde(rename = "egenmerke")]
    pub store_brand: bool,
    #[serde(rename = "verifisert")]
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct VerdictInfo {
    #[serde(rename = "verdi")]
    pub value: Verdict,
    pub l30_ore: Option<Ore>,
    pub m90_ore: Option<Ore>,
    pub atl_ore: Option<Ore>,
    #[serde(rename = "dekning_dager")]
    pub coverage_days: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
pub struct PriceRange {
    pub min_ore: Ore,
    #[serde(rename = "maks_ore")]
    pub max_ore: Ore,
}

/// One ranked row: a product at a chain (SPEC §9.1). This is the JSON contract.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SearchResult {
    #[serde(rename = "produkt")]
    pub product: ProductSummary,
    #[serde(rename = "kjede")]
    pub chain: Chain,
    #[serde(rename = "kilde")]
    pub source: SourceId,
    #[serde(rename = "antall_i_pakke")]
    pub pack_size: u32,
    #[serde(rename = "hyllepris_ore")]
    pub shelf_price: Ore,
    #[serde(rename = "medlemspris_ore")]
    pub member_price: Option<Ore>,
    #[serde(rename = "medlemsprogram")]
    pub membership_program: Option<MembershipProgram>,
    #[serde(rename = "effektiv_enhetspris_ore")]
    pub effective_unit_price: Ore,
    #[serde(rename = "literpris_ore")]
    pub liter_price: Ore,
    #[serde(rename = "minsteantall")]
    pub min_quantity: u32,
    #[serde(rename = "brukt_pris")]
    pub price_basis: PriceBasis,
    #[serde(rename = "pant_ore")]
    pub deposit: Ore,
    #[serde(rename = "pant_minsteantall_ore")]
    pub deposit_min_quantity: Ore,
    #[serde(rename = "tilbud")]
    pub offer: Option<OfferInfo>,
    #[serde(rename = "tilbudsmerke")]
    pub deal_badge: Option<DealBadge>,
    #[serde(rename = "vurdering")]
    pub verdict: VerdictInfo,
    #[serde(rename = "prisspenn")]
    pub price_range: Option<PriceRange>,
    #[serde(rename = "tilgjengelig")]
    pub available: bool,
    #[serde(rename = "sist_sett")]
    pub last_seen: Timestamp,
    #[serde(rename = "alder_timer")]
    pub age_hours: u32,
    /// The Trend column (SPEC §7.9): the liter price in a few slots over 90 days, empty
    /// without enough history. Only for the table, so it is not part of the JSON.
    #[serde(skip)]
    pub trend: Vec<Option<Ore>>,
    /// The stored listing behind the row, for `historikk`. Not part of the JSON.
    #[serde(skip)]
    pub listing_id: i64,
    /// The price interval behind the row, so an alert fires once per price. Not part of
    /// the JSON.
    #[serde(skip)]
    pub interval_id: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_slug_round_trips() {
        for chain in Chain::ALL {
            assert_eq!(Chain::from_slug(chain.slug()), Some(chain));
            // Slug, clap name and serde name must be the same.
            let clap_name = chain.to_possible_value().map(|v| v.get_name().to_owned());
            assert_eq!(clap_name.as_deref(), Some(chain.slug()));
            let json = serde_json::to_string(&chain).unwrap();
            assert_eq!(json, format!("\"{}\"", chain.slug()));
        }
    }

    #[test]
    fn verdict_label_is_the_json_name() {
        for verdict in [
            Verdict::Great,
            Verdict::Good,
            Verdict::Fair,
            Verdict::Fake,
            Verdict::Unknown,
        ] {
            let json = serde_json::to_string(&verdict).unwrap();
            assert_eq!(json, format!("\"{}\"", verdict.label()));
        }
    }

    #[test]
    fn source_slug_round_trips() {
        for source in SourceId::ALL {
            assert_eq!(SourceId::from_slug(source.slug()), Some(source));
        }
    }

    #[test]
    fn membership_slug_round_trips() {
        for program in MembershipProgram::ALL {
            assert_eq!(MembershipProgram::from_slug(program.slug()), Some(program));
            let json = serde_json::to_string(&program).unwrap();
            assert_eq!(json, format!("\"{}\"", program.slug()));
        }
    }

    #[test]
    fn container_names_are_norwegian() {
        for container in [Container::Can, Container::Bottle] {
            let clap_name = container
                .to_possible_value()
                .map(|v| v.get_name().to_owned());
            assert_eq!(clap_name.as_deref(), Some(container.slug()));
            let json = serde_json::to_string(&container).unwrap();
            assert_eq!(json, format!("\"{}\"", container.slug()));
        }
    }

    #[test]
    fn offer_serializes_as_in_spec() {
        let info = OfferInfo {
            offer: Offer::NForSum {
                n: 3,
                sum: Ore(4770),
            },
            valid_from: Some(jiff::civil::date(2026, 9, 28)),
            valid_to: Some(jiff::civil::date(2026, 10, 4)),
            source_flagged: true,
        };
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "n_for_sum", "n": 3, "sum_ore": 4770,
                "gyldig_fra": "2026-09-28", "gyldig_til": "2026-10-04", "kilde_merket": true
            })
        );
        let back: OfferInfo = serde_json::from_value(json).unwrap();
        assert_eq!(back, info);
    }

    #[test]
    fn all_offer_types_keep_their_norwegian_tags() {
        let tags = [
            (Offer::FixedPrice { price: Ore(1) }, "fastpris", "pris_ore"),
            (Offer::NForM { n: 3, m: 2 }, "n_for_m", "m"),
            (Offer::NForSum { n: 2, sum: Ore(1) }, "n_for_sum", "sum_ore"),
            (Offer::Percent { percent: 30 }, "prosent", "prosent"),
            (
                Offer::NthItem {
                    n: 3,
                    discount_percent: 100,
                },
                "nte_vare",
                "rabatt_prosent",
            ),
        ];
        for (offer, tag, field) in tags {
            let json = serde_json::to_value(&offer).unwrap();
            assert_eq!(json["type"], tag);
            assert!(json.get(field).is_some(), "{json}");
        }
    }

    #[test]
    fn ml_is_never_zero() {
        assert!(Ml::new(0).is_none());
        assert_eq!(Ml::new(500).map(Ml::get), Some(500));
    }
}
