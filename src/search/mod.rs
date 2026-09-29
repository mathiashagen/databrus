//! Filtering and ranking (SPEC §3.2, §6.4).

pub mod ranking;

use clap::ValueEnum;
use schemars::JsonSchema;
use serde::Serialize;

use crate::catalog::parse::normalize;
use crate::cli::{SearchArgs, expand_chains};
use crate::config::Config;
use crate::model::{Chain, Container, Ml, Ore, Product, SearchResult};

// The doc comments on the variants are clap's help text for `--sorter`, so they are
// Norwegian.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum, Serialize, JsonSchema)]
pub enum SortBy {
    /// Literpris uten pant
    #[default]
    #[value(name = "literpris")]
    #[serde(rename = "literpris")]
    LiterPrice,
    /// Effektiv pris per beholder
    #[value(name = "pris")]
    #[serde(rename = "pris")]
    Price,
    /// Prosent under 90-dagersmedianen
    #[value(name = "rabatt")]
    #[serde(rename = "rabatt")]
    Discount,
    /// Produktnavn
    #[value(name = "navn")]
    #[serde(rename = "navn")]
    Name,
}

/// The content of `databrus --json` next to the header (SPEC §9.1). This is the type the
/// published schema describes.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SearchContent<'a> {
    #[serde(rename = "sporring")]
    pub query: Query,
    #[serde(rename = "resultater")]
    pub results: &'a [SearchResult],
}

/// The query as echoed back in JSON (`sporring`, SPEC §9.1).
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Query {
    #[serde(rename = "tekst")]
    pub text: String,
    #[serde(rename = "filtre")]
    pub filters: Filters,
    #[serde(rename = "sorter")]
    pub sort: SortBy,
}

/// The filters in use. Unused filters are left out of the JSON.
#[derive(Debug, Clone, Default, Serialize, JsonSchema)]
pub struct Filters {
    #[serde(rename = "merke", skip_serializing_if = "Vec::is_empty")]
    pub brands: Vec<String>,
    #[serde(rename = "smak", skip_serializing_if = "Vec::is_empty")]
    pub flavors: Vec<String>,
    #[serde(rename = "storrelse_ml", skip_serializing_if = "Vec::is_empty")]
    pub sizes_ml: Vec<u32>,
    #[serde(rename = "kjede", skip_serializing_if = "Vec::is_empty")]
    pub chains: Vec<Chain>,
    #[serde(rename = "sukkerfri", skip_serializing_if = "Option::is_none")]
    pub sugar_free: Option<bool>,
    #[serde(rename = "beholder", skip_serializing_if = "Option::is_none")]
    pub container: Option<Container>,
    #[serde(rename = "maks_pris_ore", skip_serializing_if = "Option::is_none")]
    pub max_price: Option<Ore>,
    #[serde(rename = "maks_literpris_ore", skip_serializing_if = "Option::is_none")]
    pub max_liter_price: Option<Ore>,
    #[serde(rename = "enkeltvis", skip_serializing_if = "std::ops::Not::not")]
    pub single_unit: bool,
    #[serde(rename = "alle", skip_serializing_if = "std::ops::Not::not")]
    pub all: bool,
}

/// Words in the free text that turn on the sugar-free filter instead of being searched
/// for. The user types Norwegian (or English brand terms).
const SUGAR_FREE_WORDS: [&str; 4] = ["sukkerfri", "sukkerfritt", "zero", "sugarfree"];

/// A parsed search.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchFilter {
    /// Normalized words that must all appear in the product's name.
    pub words: Vec<String>,
    pub brands: Vec<String>,
    pub flavors: Vec<String>,
    pub sizes: Vec<Ml>,
    pub chains: Vec<Chain>,
    pub sugar_free: Option<bool>,
    pub container: Option<Container>,
    pub max_price: Option<Ore>,
    pub max_liter_price: Option<Ore>,
    pub sort: SortBy,
    pub single_unit: bool,
    pub all: bool,
    /// Rows shown without `--alle`.
    pub limit: usize,
}

impl SearchFilter {
    pub fn from_args(args: &SearchArgs, config: &Config) -> Self {
        let mut sugar_free = match (args.sugar_free, args.with_sugar) {
            (true, _) => Some(true),
            (_, true) => Some(false),
            _ => None,
        };
        let mut words = Vec::new();
        for word in normalize(&args.query.join(" ")).split_whitespace() {
            if SUGAR_FREE_WORDS.contains(&word) {
                sugar_free.get_or_insert(true);
            } else {
                words.push(word.to_owned());
            }
        }

        let chains = if !args.stores.is_empty() {
            expand_chains(&args.stores)
        } else if !config.default_chains.is_empty() {
            config.default_chains.clone()
        } else {
            Chain::ALL.to_vec()
        };

        Self {
            words,
            brands: args.brands.iter().map(|b| normalize(b)).collect(),
            flavors: args.flavors.iter().map(|f| normalize(f)).collect(),
            sizes: args.sizes.clone(),
            chains,
            sugar_free,
            container: args.container,
            max_price: args.max_price,
            max_liter_price: args.max_liter_price,
            sort: args.sort,
            single_unit: args.single_unit,
            all: args.all,
            limit: config.default_limit,
        }
    }

    /// The query as echoed in JSON. Only filters in use are included.
    pub fn query(&self, text: &[String]) -> Query {
        Query {
            text: text.join(" "),
            filters: Filters {
                brands: self.brands.clone(),
                flavors: self.flavors.clone(),
                sizes_ml: self.sizes.iter().map(|m| m.get()).collect(),
                chains: if self.chains.len() == Chain::ALL.len() {
                    Vec::new()
                } else {
                    self.chains.clone()
                },
                sugar_free: self.sugar_free,
                container: self.container,
                max_price: self.max_price,
                max_liter_price: self.max_liter_price,
                single_unit: self.single_unit,
                all: self.all,
            },
            sort: self.sort,
        }
    }

    pub fn chain_matches(&self, chain: Chain) -> bool {
        self.chains.contains(&chain)
    }

    /// The filters that can be decided from the catalog alone (all but chain and price).
    pub fn product_matches(&self, product: &Product) -> bool {
        if !self.brands.is_empty() && !self.brands.contains(&normalize(&product.brand)) {
            return false;
        }
        if !self.flavors.is_empty() {
            let mut flavors = std::iter::once(&product.flavor).chain(&product.flavor_aliases);
            if !flavors.any(|f| self.flavors.contains(&normalize(f))) {
                return false;
            }
        }
        if !self.sizes.is_empty() && !self.sizes.contains(&product.volume) {
            return false;
        }
        if self.sugar_free.is_some_and(|s| s != product.sugar_free) {
            return false;
        }
        if self.container.is_some_and(|c| c != product.container) {
            return false;
        }
        if self.words.is_empty() {
            return true;
        }
        // Simple substring matching. Fuzzy matching (nucleo) comes later in M1.
        let haystack = normalize(&format!(
            "{} {} {} {} {}",
            product.name,
            product.brand,
            product.line.as_deref().unwrap_or(""),
            product.flavor,
            product.flavor_aliases.join(" ")
        ));
        self.words
            .iter()
            .all(|word| haystack.contains(word.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Catalog;
    use crate::cli::ChainChoice;

    /// A fixed test catalog, so curating the built-in catalog does not break the tests.
    const TEST_CATALOG: &str = r#"
        [[produkt]]
        id = "monster-energy-500-boks"
        navn = "Monster Energy"
        merke = "monster"
        smak = "original"
        smak_alias = ["green"]
        sukkerfri = false
        volum_ml = 500
        beholder = "boks"

        [[produkt]]
        id = "monster-ultra-white-500-boks"
        navn = "Monster Ultra White"
        merke = "monster"
        smak = "ultra-white"
        sukkerfri = true
        volum_ml = 500
        beholder = "boks"

        [[produkt]]
        id = "monster-ultra-paradise-500-boks"
        navn = "Monster Ultra Paradise"
        merke = "monster"
        smak = "ultra-paradise"
        sukkerfri = true
        volum_ml = 500
        beholder = "boks"

        [[produkt]]
        id = "monster-mango-loco-500-boks"
        navn = "Monster Mango Loco"
        merke = "monster"
        smak = "mango-loco"
        smak_alias = ["mango"]
        sukkerfri = false
        volum_ml = 500
        beholder = "boks"

        [[produkt]]
        id = "red-bull-energy-drink-250-boks"
        navn = "Red Bull Energy Drink"
        merke = "red-bull"
        smak = "original"
        sukkerfri = false
        volum_ml = 250
        beholder = "boks"

        [[produkt]]
        id = "red-bull-sukkerfri-250-boks"
        navn = "Red Bull Sukkerfri"
        merke = "red-bull"
        smak = "original"
        sukkerfri = true
        volum_ml = 250
        beholder = "boks"

        [[produkt]]
        id = "nocco-miami-330-boks"
        navn = "Nocco Miami"
        merke = "nocco"
        smak = "miami"
        sukkerfri = true
        volum_ml = 330
        beholder = "boks"
    "#;

    fn matching_ids(args: SearchArgs) -> Vec<String> {
        let filter = SearchFilter::from_args(&args, &Config::default());
        Catalog::from_toml(TEST_CATALOG)
            .unwrap()
            .products
            .into_iter()
            .filter(|p| filter.product_matches(p))
            .map(|p| p.id.0)
            .collect()
    }

    #[test]
    fn free_text_and_size() {
        let ids = matching_ids(SearchArgs {
            query: vec!["monster".into(), "ultra".into()],
            sizes: vec![Ml::new(500).unwrap()],
            ..SearchArgs::default()
        });
        assert_eq!(
            ids,
            [
                "monster-ultra-white-500-boks",
                "monster-ultra-paradise-500-boks"
            ]
        );
    }

    #[test]
    fn brand_with_hyphen_and_space_is_the_same() {
        let a = matching_ids(SearchArgs {
            brands: vec!["red-bull".into()],
            ..SearchArgs::default()
        });
        let b = matching_ids(SearchArgs {
            brands: vec!["Red Bull".into()],
            ..SearchArgs::default()
        });
        assert_eq!(a.len(), 2);
        assert_eq!(a, b);
    }

    #[test]
    fn sugar_free_in_free_text_becomes_a_filter() {
        let filter = SearchFilter::from_args(
            &SearchArgs {
                query: vec!["red".into(), "bull".into(), "sukkerfri".into()],
                ..SearchArgs::default()
            },
            &Config::default(),
        );
        assert_eq!(filter.sugar_free, Some(true));
        assert_eq!(filter.words, ["red", "bull"]);
    }

    #[test]
    fn flavor_matches_alias() {
        let ids = matching_ids(SearchArgs {
            flavors: vec!["mango".into()],
            ..SearchArgs::default()
        });
        assert_eq!(ids, ["monster-mango-loco-500-boks"]);
    }

    #[test]
    fn chains_from_flag_config_or_all() {
        let mut config = Config::default();
        let without = SearchFilter::from_args(&SearchArgs::default(), &config);
        assert_eq!(without.chains, Chain::ALL);

        config.default_chains = vec![Chain::Kiwi];
        let from_config = SearchFilter::from_args(&SearchArgs::default(), &config);
        assert_eq!(from_config.chains, [Chain::Kiwi]);

        let args = SearchArgs {
            stores: vec![ChainChoice::One(Chain::Rema)],
            ..SearchArgs::default()
        };
        assert_eq!(
            SearchFilter::from_args(&args, &config).chains,
            [Chain::Rema]
        );
    }

    #[test]
    fn query_echo_has_norwegian_keys() {
        let filter = SearchFilter::from_args(
            &SearchArgs {
                sizes: vec![Ml::new(500).unwrap()],
                all: true,
                ..SearchArgs::default()
            },
            &Config::default(),
        );
        let json = serde_json::to_value(filter.query(&["monster".into()])).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "tekst": "monster",
                "filtre": {"storrelse_ml": [500], "alle": true},
                "sorter": "literpris"
            })
        );
    }
}
