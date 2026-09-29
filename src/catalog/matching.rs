//! Matching raw listings against the catalog (SPEC §6.3).
//!
//! 1. **EAN**: a listing whose EAN is in the catalog is a verified match. A listing from a
//!    source without EANs gets one through a source link (`[[kildekobling]]`) when the
//!    catalog knows the source's product id.
//! 2. **Name**: a listing *without* an EAN is matched by name, strictly, and marked
//!    unverified (`?` in the table). A listing with an EAN the catalog does not know is
//!    left unmatched: it is almost always a product that was deliberately left out
//!    (sports drinks, protein drinks), and a wrong match is worse than none.

use std::collections::{BTreeSet, HashMap};

use super::Catalog;
use super::parse::{normalize, parse_size, parse_volume};
use crate::model::{Container, Ml, Product, ProductId, RawListing, SourceId};

/// Normalizes a GTIN/EAN to 14 digits, so EAN-13 and GTIN-14 with a leading zero compare
/// equal. `None` if the string is not a plausible GTIN.
pub fn normalize_gtin(gtin: &str) -> Option<String> {
    let digits: String = gtin.chars().filter(|c| !c.is_whitespace()).collect();
    if !(8..=14).contains(&digits.len()) || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(format!("{digits:0>14}"))
}

/// A successful match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub product: ProductId,
    /// The pack size the catalog gives for the EAN. Only used as a cross-check – the
    /// listing's own name decides the pack size.
    pub pack_size: u32,
    /// `true` for EAN matches, `false` for name matches (shown with `?`).
    pub verified: bool,
}

/// Lookup from normalized EAN to product and pack size.
#[derive(Debug, Clone, Default)]
pub struct GtinIndex(HashMap<String, (ProductId, u32)>);

impl GtinIndex {
    pub fn build(catalog: &Catalog) -> Self {
        let mut index = HashMap::new();
        for product in &catalog.products {
            for gtin in &product.gtin {
                if let Some(gtin) = normalize_gtin(gtin) {
                    index.insert(gtin, (product.id.clone(), 1));
                }
            }
        }
        for pack in &catalog.multipacks {
            if let Some(gtin) = normalize_gtin(&pack.gtin) {
                index.insert(gtin, (pack.product.clone(), pack.count));
            }
        }
        Self(index)
    }

    pub fn find(&self, gtin: &str) -> Option<(&ProductId, u32)> {
        let gtin = normalize_gtin(gtin)?;
        self.0.get(&gtin).map(|(id, count)| (id, *count))
    }
}

/// Words in store names that say nothing about which product it is. The store names
/// are Norwegian.
const NOISE_WORDS: [&str; 23] = [
    // "energy"/"drink" are how stores spell "energidrikk"; "og" is "and" in "eple og pære".
    "energy",
    "drink",
    "og",
    "boks",
    "bx",
    "can",
    "flaske",
    "fl",
    "pet",
    "energidrikk",
    "energi",
    "drikk",
    "stk",
    "pk",
    "pakning",
    "x",
    "l",
    "ml",
    "cl",
    "inkl",
    "pant",
    "mack",
    "brett",
];

/// Words that mean "sugar-free". Left over in a name, they are only allowed when the
/// product is sugar-free.
const SUGAR_FREE_WORDS: [&str; 10] = [
    "sukkerfri",
    "sukkerfritt",
    "sugarfree",
    "zero",
    "sugar",
    "free",
    "u",
    "uten",
    "sukker",
    "nocal",
];

const BOTTLE_WORDS: [&str; 3] = ["flaske", "fl", "pet"];

/// A set of words that identifies a product.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Variant {
    words: BTreeSet<String>,
    /// The name and hand-written aliases are strong; the flavor slug, generated from the
    /// name, is weak and only wins when nothing strong matches.
    strong: bool,
}

/// How well a product matches: strong before weak, then more matched words.
type Score = (bool, usize);

/// One catalog product, prepared for name matching.
#[derive(Debug, Clone)]
struct NameEntry {
    product: ProductId,
    volume: Ml,
    container: Container,
    sugar_free: bool,
    /// The brand as words, e.g. `["red", "bull"]`.
    brand: Vec<String>,
    /// Alternative word sets that identify the product. One of them must be fully present
    /// in the listing.
    variants: Vec<Variant>,
    /// Every word the product can explain.
    explained: BTreeSet<String>,
}

impl NameEntry {
    fn new(product: &Product) -> Self {
        let brand: Vec<String> = words(&product.brand);
        let without_brand = |text: &str| -> BTreeSet<String> {
            let mut w = words(text);
            remove_phrase(&mut w, &brand);
            w.into_iter().filter(|w| !is_noise(w)).collect()
        };
        let strong = |text: &str| Variant {
            words: without_brand(text),
            strong: true,
        };
        let mut variants = vec![strong(&product.name)];
        variants.extend(product.flavor_aliases.iter().map(|a| strong(a)));
        variants.push(Variant {
            words: without_brand(&product.flavor),
            strong: false,
        });
        let explained = variants.iter().flat_map(|v| v.words.clone()).collect();
        Self {
            product: product.id.clone(),
            volume: product.volume,
            container: product.container,
            sugar_free: product.sugar_free,
            brand,
            variants,
            explained,
        }
    }

    /// How well the listing matches, or `None` if it does not.
    fn score(&self, listing: &ListingWords) -> Option<Score> {
        if listing.volume != self.volume || (listing.bottle && self.container != Container::Bottle)
        {
            return None;
        }
        let mut words = listing.words.clone();
        let brand_in_name = remove_phrase(&mut words, &self.brand);
        if !brand_in_name && listing.brand.as_deref() != Some(self.brand.as_slice()) {
            return None;
        }
        let words: BTreeSet<String> = words.into_iter().filter(|w| !is_noise(w)).collect();

        let best = self
            .variants
            .iter()
            .filter(|v| v.words.is_subset(&words))
            .map(|v| (v.strong, v.words.len()))
            .max()?;
        // Every word must be explained: by the product, or by being a sugar-free word
        // on a sugar-free product.
        let all_explained = words.iter().all(|w| {
            self.explained.contains(w)
                || (self.sugar_free && SUGAR_FREE_WORDS.contains(&w.as_str()))
        });
        all_explained.then_some(best)
    }
}

/// A listing's name, prepared for matching.
struct ListingWords {
    words: Vec<String>,
    brand: Option<Vec<String>>,
    volume: Ml,
    bottle: bool,
}

impl ListingWords {
    fn new(listing: &RawListing) -> Option<Self> {
        // The size may be in the name, in free text in the size field ("Blåbær, 250 ml"),
        // or be a bare number in the size field ("473").
        let size = listing.raw_size.as_deref();
        let volume = parse_volume(&listing.raw_name)
            .or_else(|| size.and_then(parse_volume))
            .or_else(|| size.and_then(|s| parse_size(s).ok()))?;
        let name_words = words(&listing.raw_name);
        let bottle = name_words
            .iter()
            .any(|w| BOTTLE_WORDS.contains(&w.as_str()));
        Some(Self {
            words: name_words,
            brand: listing.raw_brand.as_deref().map(words),
            volume,
            bottle,
        })
    }
}

/// Name lookup for listings without an EAN.
#[derive(Debug, Clone, Default)]
pub struct NameIndex(Vec<NameEntry>);

impl NameIndex {
    pub fn build(catalog: &Catalog) -> Self {
        Self(catalog.products.iter().map(NameEntry::new).collect())
    }

    /// The single best product for the listing's name, or `None` if nothing matches or
    /// two products match equally well.
    pub fn find(&self, listing: &RawListing) -> Option<&ProductId> {
        let listing_words = ListingWords::new(listing)?;
        let mut best: Option<(Score, &ProductId)> = None;
        let mut tied = false;
        for entry in &self.0 {
            let Some(score) = entry.score(&listing_words) else {
                continue;
            };
            match best {
                Some((top, _)) if score < top => {}
                Some((top, _)) if score == top => tied = true,
                _ => {
                    best = Some((score, &entry.product));
                    tied = false;
                }
            }
        }
        if tied {
            tracing::debug!(
                "«{}»: flere like gode navnetreff – matcher ikke",
                listing.raw_name
            );
            return None;
        }
        best.map(|(_, product)| product)
    }
}

/// EAN and name lookup together.
#[derive(Debug, Clone, Default)]
pub struct Matcher {
    gtins: GtinIndex,
    names: NameIndex,
    /// (source, source product id) → normalized EAN, from the catalog's source links.
    links: HashMap<(SourceId, String), String>,
}

impl Matcher {
    pub fn build(catalog: &Catalog) -> Self {
        let links = catalog
            .source_links
            .iter()
            .filter_map(|l| Some(((l.source, l.id.clone()), normalize_gtin(&l.gtin)?)))
            .collect();
        Self {
            gtins: GtinIndex::build(catalog),
            names: NameIndex::build(catalog),
            links,
        }
    }

    /// The listing's EAN: its own, or one from a source link.
    fn gtin<'a>(&'a self, listing: &'a RawListing) -> Option<&'a str> {
        listing
            .gtin
            .as_deref()
            .filter(|g| normalize_gtin(g).is_some())
            .or_else(|| {
                self.links
                    .get(&(listing.source, listing.source_product_id.clone()))
                    .map(String::as_str)
            })
    }

    /// EAN first; name only for listings without a valid EAN (see the module docs).
    pub fn find(&self, listing: &RawListing) -> Option<Match> {
        match self.gtin(listing) {
            Some(gtin) => {
                let (product, pack_size) = self.gtins.find(gtin)?;
                Some(Match {
                    product: product.clone(),
                    pack_size,
                    verified: true,
                })
            }
            None => self.names.find(listing).map(|product| Match {
                product: product.clone(),
                pack_size: 1,
                verified: false,
            }),
        }
    }
}

/// Normalized words, without tokens that start with a digit (sizes and pack sizes like
/// "0", "5l", "24x500ml", "4pk").
fn words(text: &str) -> Vec<String> {
    normalize(text)
        .split_whitespace()
        .filter(|w| !w.starts_with(|c: char| c.is_ascii_digit()))
        .map(str::to_owned)
        .collect()
}

fn is_noise(word: &str) -> bool {
    NOISE_WORDS.contains(&word)
}

/// Removes the first occurrence of `phrase` as consecutive words. Returns whether it was
/// found. An empty phrase is never found.
fn remove_phrase(words: &mut Vec<String>, phrase: &[String]) -> bool {
    if phrase.is_empty() || phrase.len() > words.len() {
        return false;
    }
    let found = (0..=words.len() - phrase.len()).find(|&i| words[i..i + phrase.len()] == *phrase);
    match found {
        Some(i) => {
            words.drain(i..i + phrase.len());
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Chain, Ore, SourceId};

    #[test]
    fn gtin_is_normalized_to_14_digits() {
        assert_eq!(
            normalize_gtin("7040110569908").as_deref(),
            Some("07040110569908")
        );
        assert_eq!(
            normalize_gtin("07040110569908"),
            normalize_gtin("7040110569908")
        );
        assert_eq!(normalize_gtin("12345"), None);
        assert_eq!(normalize_gtin("70401105699O8"), None);
    }

    #[test]
    fn index_finds_singles_and_multipacks() {
        let catalog = Catalog::from_toml(
            r#"
            [[produkt]]
            id = "p"
            navn = "P"
            merke = "x"
            smak = "a"
            sukkerfri = false
            volum_ml = 500
            beholder = "boks"
            gtin = ["7040110569908"]

            [[flerpakning]]
            gtin = "7040110569915"
            produkt = "p"
            antall = 4
            "#,
        )
        .unwrap();
        let index = GtinIndex::build(&catalog);
        let p = ProductId("p".into());
        assert_eq!(index.find("07040110569908"), Some((&p, 1)));
        assert_eq!(index.find("7040110569915"), Some((&p, 4)));
        assert_eq!(index.find("7040110569922"), None);
    }

    fn listing(name: &str, gtin: Option<&str>) -> RawListing {
        RawListing {
            source: SourceId::Kassalapp,
            chain: Chain::Engrossnett,
            source_product_id: "1".into(),
            gtin: gtin.map(str::to_owned),
            raw_name: name.into(),
            raw_brand: None,
            raw_size: None,
            pack_size: 1,
            shelf_price: Ore(1000),
            member_price: None,
            offer: None,
            available: None,
            deposit: None,
            source_timestamp: None,
        }
    }

    fn by_name(name: &str) -> Option<String> {
        let matcher = Matcher::build(&Catalog::builtin().unwrap());
        matcher.find(&listing(name, None)).map(|m| {
            assert!(!m.verified, "name matches are unverified");
            m.product.0
        })
    }

    /// The real barcode-less Engrossnett names from Kassalapp (2026-09-29).
    #[test]
    fn real_listings_without_ean_are_matched_by_name() {
        let cases = [
            ("Burn Fruit Punch Boks 12x0,5l", "burn-fruit-punch-500-boks"),
            (
                "Monster Juiced Khaotic 24x500ml",
                "monster-juiced-khaotic-500-boks",
            ),
            (
                "Monster Juiced Monarch 24x500ml",
                "monster-juiced-monarch-500-boks",
            ),
            (
                "Monster LH44 Zero 24x500ml",
                "monster-full-throttle-zero-500-boks",
            ),
            ("Monster Mule 24x500ml", "monster-mule-500-boks"),
            (
                "Monster Nitro Super Dry 24x500ml",
                "monster-nitro-super-dry-500-boks",
            ),
            (
                "Monster Ultra Fiesta 24x500ml",
                "monster-ultra-fiesta-500-boks",
            ),
            ("Monster Ultra Rosa 24x500ml", "monster-ultra-rosa-500-boks"),
            (
                "Monster Ultra Watermelon Zero 24x500ml",
                "monster-ultra-watermelon-500-boks",
            ),
            (
                "Red Bull Regular 24x473ml",
                "red-bull-energy-drink-473-boks",
            ),
            (
                "Red Bull Yellow Edition 24x0,25l",
                "red-bull-yellow-edition-250-boks",
            ),
        ];
        for (name, expected) in cases {
            assert_eq!(by_name(name).as_deref(), Some(expected), "{name}");
        }
    }

    #[test]
    fn sugar_free_words_pick_the_sugar_free_variant() {
        assert_eq!(
            by_name("Tørst Energidrikk 0,5l").as_deref(),
            Some("torst-500-boks")
        );
        assert_eq!(
            by_name("Tørst Energidrikk Sukkerfri 0,5l").as_deref(),
            Some("torst-sukkerfri-500-boks")
        );
        // "zero" is not allowed on a product with sugar.
        assert_eq!(by_name("Monster Mango Loco Zero 0,5l"), None);
    }

    #[test]
    fn products_that_are_not_in_the_catalog_stay_unmatched() {
        for name in [
            "Powerade Golden Mango 0,5l flaske",
            "Prime Blue Raspberry 500 ml",
            "Maxim Energy Gel Orange 33g",
            "Monster Mango Loco 1/4pl",
            // Right brand, but a flavor the catalog does not have.
            "Monster Ultra Blue Hawaiian 0,5l",
            // Right product, wrong size.
            "Monster Ultra White 0,355l",
        ] {
            assert_eq!(by_name(name), None, "{name}");
        }
    }

    #[test]
    fn a_bottle_never_matches_a_can() {
        assert_eq!(by_name("Monster Energy 0,5l flaske"), None);
    }

    #[test]
    fn an_unknown_ean_is_not_matched_by_name() {
        let matcher = Matcher::build(&Catalog::builtin().unwrap());
        let unknown = listing("Monster Ultra White 0,5l boks", Some("7090000000017"));
        assert_eq!(matcher.find(&unknown), None);
        // Without the EAN the same name matches.
        assert!(
            matcher
                .find(&listing("Monster Ultra White 0,5l boks", None))
                .is_some()
        );
    }

    #[test]
    fn a_tie_between_two_products_is_not_a_match() {
        // Two products that both claim "mango" by name or alias.
        let catalog = Catalog::from_toml(
            r#"
            [[produkt]]
            id = "a"
            navn = "Brus Mango"
            merke = "brus"
            smak = "mango"
            sukkerfri = false
            volum_ml = 500
            beholder = "boks"

            [[produkt]]
            id = "b"
            navn = "Brus Mango Classic"
            merke = "brus"
            smak = "mango"
            sukkerfri = false
            volum_ml = 500
            beholder = "boks"

            [[produkt]]
            id = "c"
            navn = "Brus Tropisk"
            merke = "brus"
            smak = "tropisk"
            smak_alias = ["mango"]
            sukkerfri = false
            volum_ml = 500
            beholder = "boks"
            "#,
        )
        .unwrap();
        let matcher = Matcher::build(&catalog);
        let find = |name: &str| matcher.find(&listing(name, None)).map(|m| m.product.0);
        // "b" only matches "Brus Mango" through its (weak) flavor, but "a" and "c" both
        // match through their name or alias – a genuine tie.
        assert_eq!(find("Brus Mango 0,5l"), None);
        // The longer name wins when all its words are present.
        assert_eq!(find("Brus Mango Classic 0,5l").as_deref(), Some("b"));
        assert_eq!(find("Brus Tropisk 0,5l").as_deref(), Some("c"));
    }

    #[test]
    fn a_source_link_gives_a_verified_match() {
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

            [[kildekobling]]
            kilde = "oda"
            id = "23300"
            gtin = "5060166693732"

            [[kildekobling]]
            kilde = "oda"
            id = "7957"
            gtin = "54492653"
            "#,
        )
        .unwrap();
        let matcher = Matcher::build(&catalog);
        let oda = |id: &str, name: &str| {
            let mut l = listing(name, None);
            l.source = SourceId::Oda;
            l.source_product_id = id.into();
            matcher.find(&l)
        };
        // The name alone would not match ("Grønn" is not an alias here), the link does.
        let found = oda("23300", "Monster Grønn 0,5 l").unwrap();
        assert_eq!(found.product.0, "monster-energy-500-boks");
        assert!(found.verified);
        // A link to an EAN outside the catalog keeps the listing unmatched – it is not
        // matched by name either.
        assert_eq!(oda("7957", "Monster Energy 0,5 l"), None);
        // Another source with the same id is not linked.
        let mut kassalapp = listing("Monster Grønn 0,5 l", None);
        kassalapp.source_product_id = "23300".into();
        assert_eq!(matcher.find(&kassalapp), None);
    }

    #[test]
    fn the_size_can_be_free_text_in_the_size_field() {
        let mut l = listing("Monster Ultra Rosa", None);
        l.raw_size = Some("Rosa, 0,5 l".into());
        let matcher = Matcher::build(&Catalog::builtin().unwrap());
        assert_eq!(
            matcher.find(&l).map(|m| m.product.0).as_deref(),
            Some("monster-ultra-rosa-500-boks")
        );
    }

    #[test]
    fn brand_can_come_from_the_source() {
        let mut l = listing("Ultra Rosa 0,5l", None);
        l.raw_brand = Some("Monster".into());
        let matcher = Matcher::build(&Catalog::builtin().unwrap());
        assert_eq!(
            matcher.find(&l).map(|m| m.product.0).as_deref(),
            Some("monster-ultra-rosa-500-boks")
        );
    }
}
