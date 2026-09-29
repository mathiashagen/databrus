//! Configuration (SPEC §10) and file locations (SPEC §7.1).
//!
//! The Rust names are English; the keys in `konfig.toml` are Norwegian (`serde(rename)`).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use clap::ValueEnum;
use directories::BaseDirs;
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::history::Thresholds;
use crate::model::{Chain, MembershipProgram, SourceId};
use crate::pricing::DepositConfig;

/// Overrides the data directory (used by tests and portable setups).
pub const ENV_DATA_DIR: &str = "DATABRUS_DATA_DIR";
/// The Kassalapp key in the environment takes precedence over the config file (SPEC §10.2).
pub const ENV_API_KEY: &str = "KASSALAPP_API_KEY";
/// The floor for `henting.min_intervall_minutter` (SPEC §7.4).
pub const MIN_INTERVAL_FLOOR: u32 = 15;

/// The commented file `konfig init` writes. Must parse to [`Config::default`].
pub const DEFAULT_TOML: &str = r#"# databrus – konfigurasjon (se SPEC.md §10)

# Lojalitetsprogrammer du er med i. Medlemspriser brukes i rangeringen
# bare for disse: coop | trumf | ae | kiwi-pluss
medlemskap = []

# Kjeder som brukes når --butikk ikke er gitt. Tom liste betyr alle kjeder.
standard_butikker = []

# Antall rader i tabellen når --alle ikke er gitt.
standard_antall = 20

# Farger: auto | alltid | aldri
farge = "auto"

[henting]
# Hvor lenge data fra en kilde regnes som ferske.
ttl_timer = 6
# Minste tid mellom to hentinger fra samme kilde. Kan ikke settes lavere enn 15.
min_intervall_minutter = 15

[pant]
liten_ore = 200   # beholdere til og med grense_ml
stor_ore = 300    # beholdere over grense_ml
grense_ml = 500

[vurdering]
min_dekning_dager = 14
supert_under_median_prosent = 20
bra_under_median_prosent = 10
atl_toleranse_prosent = 2
lureri_prisokning_prosent = 5
prisfall_prosent = 10

[kilder.kassalapp]
aktiv = true
# Gratis nøkkel fra https://kassal.app/api. Miljøvariabelen KASSALAPP_API_KEY går foran.
api_nokkel = ""

[kilder.oda]
aktiv = true

[kilder.rema]
aktiv = true

[kilder.coop]
aktiv = true
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum, Serialize, Deserialize)]
pub enum ColorMode {
    #[default]
    #[value(name = "auto")]
    #[serde(rename = "auto")]
    Auto,
    #[value(name = "alltid")]
    #[serde(rename = "alltid")]
    Always,
    #[value(name = "aldri")]
    #[serde(rename = "aldri")]
    Never,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    #[serde(rename = "medlemskap")]
    pub memberships: Vec<MembershipProgram>,
    #[serde(rename = "standard_butikker")]
    pub default_chains: Vec<Chain>,
    #[serde(rename = "standard_antall")]
    pub default_limit: usize,
    #[serde(rename = "farge")]
    pub color: ColorMode,
    #[serde(rename = "henting")]
    pub fetching: Fetching,
    #[serde(rename = "pant")]
    pub deposit: DepositConfig,
    #[serde(rename = "vurdering")]
    pub verdict: Thresholds,
    #[serde(rename = "kilder")]
    pub sources: Sources,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            memberships: Vec::new(),
            default_chains: Vec::new(),
            default_limit: 20,
            color: ColorMode::Auto,
            fetching: Fetching::default(),
            deposit: DepositConfig::default(),
            verdict: Thresholds::default(),
            sources: Sources::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Fetching {
    #[serde(rename = "ttl_timer")]
    pub ttl_hours: u32,
    #[serde(rename = "min_intervall_minutter")]
    pub min_interval_minutes: u32,
}

impl Default for Fetching {
    fn default() -> Self {
        Self {
            ttl_hours: 6,
            min_interval_minutes: MIN_INTERVAL_FLOOR,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sources {
    pub kassalapp: KassalappConfig,
    pub oda: SourceConfig,
    pub rema: SourceConfig,
    pub coop: SourceConfig,
}

impl Sources {
    pub fn is_enabled(&self, source: SourceId) -> bool {
        match source {
            SourceId::Kassalapp => self.kassalapp.enabled,
            SourceId::Oda => self.oda.enabled,
            SourceId::Rema => self.rema.enabled,
            SourceId::Coop => self.coop.enabled,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SourceConfig {
    #[serde(rename = "aktiv")]
    pub enabled: bool,
}

impl Default for SourceConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KassalappConfig {
    #[serde(rename = "aktiv")]
    pub enabled: bool,
    #[serde(rename = "api_nokkel")]
    pub api_key: String,
}

impl Default for KassalappConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            api_key: String::new(),
        }
    }
}

impl Config {
    /// Reads the configuration. A missing file gives the defaults.
    pub fn load(path: &Path) -> Result<Self, AppError> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(error.into()),
        };
        let config: Config = toml::from_str(&text)
            .map_err(|error| AppError::Config(format!("{}: {error}", path.display())))?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), AppError> {
        if self.fetching.min_interval_minutes < MIN_INTERVAL_FLOOR {
            return Err(AppError::Config(format!(
                "henting.min_intervall_minutter må være minst {MIN_INTERVAL_FLOOR}"
            )));
        }
        if self.fetching.ttl_hours == 0 {
            return Err(AppError::Config("henting.ttl_timer må være minst 1".into()));
        }
        if self.default_limit == 0 {
            return Err(AppError::Config("standard_antall må være minst 1".into()));
        }
        if self.deposit.threshold_ml == 0 || self.deposit.small.0 < 0 || self.deposit.large.0 < 0 {
            return Err(AppError::Config("ugyldige pantesatser".into()));
        }
        Ok(())
    }

    /// The Kassalapp key: the environment variable first, then the config file.
    pub fn api_key(&self) -> Option<String> {
        std::env::var(ENV_API_KEY)
            .ok()
            .filter(|k| !k.trim().is_empty())
            .or_else(|| {
                let key = self.sources.kassalapp.api_key.trim();
                (!key.is_empty()).then(|| key.to_owned())
            })
    }

    /// A copy that is safe to show: the API key is masked.
    pub fn redacted(&self) -> Self {
        let mut copy = self.clone();
        if !copy.sources.kassalapp.api_key.is_empty() {
            copy.sources.kassalapp.api_key = "***".into();
        }
        copy
    }
}

/// Sets one value in the config file, e.g. `henting.ttl_timer = 12`.
///
/// `key` is the Norwegian key path. It must exist in the schema, and the result must
/// validate. Note: the file is rewritten without comments.
pub fn set_value(path: &Path, key: &str, value: &str) -> Result<(), AppError> {
    let defaults = toml::Table::try_from(Config::default())
        .map_err(|error| AppError::Config(error.to_string()))?;
    if !key_exists(&defaults, key) {
        return Err(AppError::Config(format!("ukjent nøkkel «{key}»")));
    }

    let mut table: toml::Table = match fs::read_to_string(path) {
        Ok(text) => toml::from_str(&text)
            .map_err(|error| AppError::Config(format!("{}: {error}", path.display())))?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => toml::Table::new(),
        Err(error) => return Err(error.into()),
    };
    set_in_table(&mut table, key, parse_value(value));

    let config: Config = toml::Value::Table(table.clone())
        .try_into()
        .map_err(|error| AppError::Config(format!("ugyldig verdi for {key}: {error}")))?;
    config.validate()?;

    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let text =
        toml::to_string_pretty(&table).map_err(|error| AppError::Config(error.to_string()))?;
    fs::write(path, text)?;
    Ok(())
}

fn key_exists(table: &toml::Table, key: &str) -> bool {
    let mut current = table;
    let mut parts = key.split('.').peekable();
    while let Some(part) = parts.next() {
        match (current.get(part), parts.peek()) {
            (Some(_), None) => return true,
            (Some(toml::Value::Table(inner)), Some(_)) => current = inner,
            _ => return false,
        }
    }
    false
}

/// Parses the value as TOML (`12`, `true`, `["coop"]`), otherwise as a string.
fn parse_value(value: &str) -> toml::Value {
    toml::from_str::<toml::Table>(&format!("v = {value}"))
        .ok()
        .and_then(|mut t| t.remove("v"))
        .unwrap_or_else(|| toml::Value::String(value.to_owned()))
}

fn set_in_table(table: &mut toml::Table, key: &str, value: toml::Value) {
    match key.split_once('.') {
        None => {
            table.insert(key.to_owned(), value);
        }
        Some((head, rest)) => {
            let inner = table
                .entry(head.to_owned())
                .or_insert_with(|| toml::Value::Table(toml::Table::new()));
            if !inner.is_table() {
                *inner = toml::Value::Table(toml::Table::new());
            }
            if let toml::Value::Table(inner) = inner {
                set_in_table(inner, rest, value);
            }
        }
    }
}

/// Where the files live (SPEC §7.1).
#[derive(Debug, Clone)]
pub struct Paths {
    pub config_file: PathBuf,
    pub data_dir: PathBuf,
}

impl Paths {
    /// `config_file` comes from `--konfig` / `DATABRUS_KONFIG` when set.
    pub fn find(config_file: Option<&Path>) -> Result<Self, AppError> {
        let base = BaseDirs::new();
        let config_file = match (config_file, &base) {
            (Some(path), _) => path.to_path_buf(),
            (None, Some(b)) => b.config_dir().join("databrus").join("konfig.toml"),
            (None, None) => return Err(no_home_dir()),
        };
        let data_dir = match (std::env::var_os(ENV_DATA_DIR), &base) {
            (Some(path), _) if !path.is_empty() => PathBuf::from(path),
            (_, Some(b)) => b.data_local_dir().join("databrus"),
            (_, None) => return Err(no_home_dir()),
        };
        Ok(Self {
            config_file,
            data_dir,
        })
    }

    pub fn database(&self) -> PathBuf {
        self.data_dir.join("databrus.sqlite")
    }

    /// The user's catalog override lives next to the config file.
    pub fn catalog_override(&self) -> PathBuf {
        self.config_file
            .parent()
            .map_or_else(|| PathBuf::from("katalog.toml"), |d| d.join("katalog.toml"))
    }
}

fn no_home_dir() -> AppError {
    AppError::Config(format!(
        "fant ingen hjemmemappe – bruk --konfig og {ENV_DATA_DIR}"
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn default_file_gives_the_defaults() {
        let config: Config = toml::from_str(DEFAULT_TOML).unwrap();
        assert_eq!(config, Config::default());
        config.validate().unwrap();
    }

    /// Unknown keys are ignored when reading, so a typo in a `serde(rename)` would go
    /// unnoticed by the test above. Compare the key sets directly.
    #[test]
    fn default_file_keys_match_the_struct() {
        fn keys(table: &toml::Table, prefix: &str, out: &mut BTreeSet<String>) {
            for (key, value) in table {
                let path = format!("{prefix}{key}");
                match value {
                    toml::Value::Table(inner) => keys(inner, &format!("{path}."), out),
                    _ => {
                        out.insert(path);
                    }
                }
            }
        }
        let mut from_file = BTreeSet::new();
        keys(&toml::from_str(DEFAULT_TOML).unwrap(), "", &mut from_file);
        let mut from_struct = BTreeSet::new();
        keys(
            &toml::Table::try_from(Config::default()).unwrap(),
            "",
            &mut from_struct,
        );
        assert_eq!(from_file, from_struct);
    }

    #[test]
    fn empty_file_gives_the_defaults() {
        let config: Config = toml::from_str("").unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn min_interval_below_floor_is_rejected() {
        let config: Config = toml::from_str("[henting]\nmin_intervall_minutter = 5").unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn values_from_the_spec_parse() {
        let config: Config = toml::from_str(
            "medlemskap = [\"coop\", \"kiwi-pluss\"]\nstandard_butikker = [\"coop-extra\"]",
        )
        .unwrap();
        assert_eq!(
            config.memberships,
            [MembershipProgram::Coop, MembershipProgram::KiwiPluss]
        );
        assert_eq!(config.default_chains, [Chain::CoopExtra]);
    }

    #[test]
    fn set_value_writes_and_validates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("konfig.toml");

        set_value(&path, "henting.ttl_timer", "12").unwrap();
        set_value(&path, "medlemskap", "[\"trumf\"]").unwrap();
        let config = Config::load(&path).unwrap();
        assert_eq!(config.fetching.ttl_hours, 12);
        assert_eq!(config.memberships, [MembershipProgram::Trumf]);

        assert!(set_value(&path, "henting.min_intervall_minutter", "5").is_err());
        assert!(set_value(&path, "finnes.ikke", "1").is_err());
        assert!(set_value(&path, "farge", "rosa").is_err());
        // A rejected value must not have changed the file.
        assert_eq!(
            Config::load(&path).unwrap().fetching.min_interval_minutes,
            15
        );
    }

    #[test]
    fn redacted_masks_the_api_key() {
        let mut config = Config::default();
        config.sources.kassalapp.api_key = "hemmelig".into();
        assert_eq!(config.redacted().sources.kassalapp.api_key, "***");
    }
}
