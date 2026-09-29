//! The clap definitions. Only structs here – the logic lives in [`crate::commands`].
//!
//! The field names are English, but every command, flag and value has an explicit
//! Norwegian name, and the doc comments are the Norwegian help text (SPEC §2). Command
//! and flag names are ASCII (æ→ae, ø→o, å→a).

use std::path::PathBuf;

use clap::builder::PossibleValue;
use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;
use jiff::civil::{Date, Time};

use crate::catalog::parse::parse_size;
use crate::config::ColorMode;
use crate::model::{Chain, Container, Ml, Ore, SourceId};
use crate::output::format::parse_kr;
use crate::search::SortBy;

// Without a command, free text and filters are a search: `databrus monster ultra
// --storrelse 0,5`. A command name only counts as a command when no free text comes
// before it: `databrus monster tilbud` searches for "monster tilbud", and
// `databrus sok tilbud` searches for "tilbud". Filters before a command are rejected in
// `parse_from` (they belong after the command).
#[derive(Debug, Parser)]
#[command(
    name = "databrus",
    bin_name = "databrus",
    version,
    about = "Finn de billigste energidrikkene i Norge",
    override_usage = "databrus [VALG] [SØK]...\n      databrus <KOMMANDO> [VALG]",
    disable_version_flag = true,
    disable_help_subcommand = true,
    subcommand_help_heading = "Kommandoer",
    subcommand_value_name = "KOMMANDO"
)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalArgs,

    #[command(flatten)]
    pub search: SearchArgs,

    #[command(subcommand)]
    pub command: Option<Command>,
}

/// Options that apply to every command (SPEC §3.3).
#[derive(Debug, Clone, Args)]
#[command(next_help_heading = "Globale valg")]
pub struct GlobalArgs {
    /// Skriv ut et versjonert JSON-dokument
    #[arg(long = "json", global = true, conflicts_with = "json_lines")]
    pub json: bool,

    /// Skriv ut NDJSON: én hodelinje, deretter én linje per rad
    #[arg(long = "json-linjer", global = true)]
    pub json_lines: bool,

    /// Hent ferske priser før svar (tidligst 15 min etter forrige henting)
    #[arg(long = "oppdater", global = true, conflicts_with = "offline")]
    pub refresh: bool,

    /// Bruk bare lokale data – aldri nettverket
    #[arg(long = "frakoblet", global = true)]
    pub offline: bool,

    /// Avslutt med kode 2 hvis en kilde feilet eller dataene er utdaterte
    #[arg(long = "streng", global = true)]
    pub strict: bool,

    /// Farger i utdata
    #[arg(long = "farge", global = true, value_enum, value_name = "NÅR")]
    pub color: Option<ColorMode>,

    /// Mer diagnostikk på stderr (-v, -vv)
    #[arg(short = 'v', long = "detaljert", action = ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Bruk en annen konfigurasjonsfil
    #[arg(
        long = "konfig",
        global = true,
        env = "DATABRUS_KONFIG",
        hide_env_values = true,
        value_name = "STI"
    )]
    pub config: Option<PathBuf>,

    /// Vis hjelp
    #[arg(short = 'h', long = "help", action = ArgAction::Help, global = true)]
    pub show_help: Option<bool>,

    /// Vis versjon
    #[arg(short = 'V', long = "version", action = ArgAction::Version)]
    pub show_version: Option<bool>,
}

/// Search filters, shared by search, `tilbud`, `produkter` and `eksporter` (SPEC §3.2).
#[derive(Debug, Clone, Default, Args)]
#[command(next_help_heading = "Filtre")]
pub struct SearchArgs {
    /// Fritekst, f.eks. «monster ultra»
    #[arg(value_name = "SØK")]
    pub query: Vec<String>,

    /// Merke, f.eks. monster, red-bull, nocco (kan gjentas)
    #[arg(long = "merke", value_name = "MERKE")]
    pub brands: Vec<String>,

    /// Smak (kan gjentas)
    #[arg(long = "smak", value_name = "SMAK")]
    pub flavors: Vec<String>,

    /// Størrelse: 0,5 · 500ml · 0,33l (kan gjentas, ikke kommaliste)
    #[arg(long = "storrelse", value_name = "STR", value_parser = parse_size)]
    pub sizes: Vec<Ml>,

    /// Kjeder, f.eks. kiwi,rema – «coop» betyr alle Coop-kjedene
    #[arg(
        long = "butikk",
        value_name = "KJEDE",
        value_enum,
        value_delimiter = ','
    )]
    pub stores: Vec<ChainChoice>,

    /// Bare sukkerfrie
    #[arg(long = "sukkerfri", conflicts_with = "with_sugar")]
    pub sugar_free: bool,

    /// Bare med sukker
    #[arg(long = "med-sukker")]
    pub with_sugar: bool,

    /// Beholdertype
    #[arg(long = "beholder", value_enum, value_name = "BEHOLDER")]
    pub container: Option<Container>,

    /// Høyeste effektive pris per beholder, i kr
    #[arg(long = "maks-pris", value_name = "KR", value_parser = parse_kr)]
    pub max_price: Option<Ore>,

    /// Høyeste literpris, i kr
    #[arg(long = "maks-literpris", value_name = "KR", value_parser = parse_kr)]
    pub max_liter_price: Option<Ore>,

    /// Sortering
    #[arg(long = "sorter", value_enum, value_name = "SORTER", default_value_t = SortBy::LiterPrice)]
    pub sort: SortBy,

    /// Ranger etter pris for én beholder og se bort fra flerkjøpstilbud
    #[arg(long = "enkeltvis")]
    pub single_unit: bool,

    /// Vis alle rader, også utsolgte og svært gamle
    #[arg(long = "alle")]
    pub all: bool,
}

/// A chain choice on the command line: one chain, or `coop` for all Coop chains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainChoice {
    One(Chain),
    AllCoop,
}

const CHAIN_CHOICES: [ChainChoice; 16] = [
    ChainChoice::One(Chain::Rema),
    ChainChoice::One(Chain::Kiwi),
    ChainChoice::One(Chain::Meny),
    ChainChoice::One(Chain::Spar),
    ChainChoice::One(Chain::Joker),
    ChainChoice::AllCoop,
    ChainChoice::One(Chain::CoopExtra),
    ChainChoice::One(Chain::CoopObs),
    ChainChoice::One(Chain::CoopMega),
    ChainChoice::One(Chain::CoopPrix),
    ChainChoice::One(Chain::Bunnpris),
    ChainChoice::One(Chain::Oda),
    ChainChoice::One(Chain::Europris),
    ChainChoice::One(Chain::Engrossnett),
    ChainChoice::One(Chain::Havaristen),
    ChainChoice::One(Chain::Fastcandy),
];

impl ValueEnum for ChainChoice {
    fn value_variants<'a>() -> &'a [Self] {
        &CHAIN_CHOICES
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        Some(match self {
            ChainChoice::One(chain) => PossibleValue::new(chain.slug()),
            ChainChoice::AllCoop => PossibleValue::new("coop").help("alle Coop-kjedene"),
        })
    }
}

/// Turns chain choices into a sorted list without duplicates.
pub fn expand_chains(choices: &[ChainChoice]) -> Vec<Chain> {
    let mut chains: Vec<Chain> = choices
        .iter()
        .flat_map(|c| match c {
            ChainChoice::One(chain) => vec![*chain],
            ChainChoice::AllCoop => Chain::COOP.to_vec(),
        })
        .collect();
    chains.sort();
    chains.dedup();
    chains
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Søk etter energidrikker (det samme som uten kommando)
    #[command(name = "sok")]
    Search(SearchArgs),

    /// Vis aktuelle tilbud, sortert etter hvor gode de er
    #[command(name = "tilbud")]
    Deals(DealsArgs),

    /// Vis prishistorikk for et produkt
    #[command(name = "historikk")]
    History(HistoryArgs),

    /// Hent priser fra alle kilder nå
    #[command(name = "oppdater")]
    Update(UpdateArgs),

    /// Vis kjeder, kilder og hvor ferske dataene er
    #[command(name = "butikker")]
    Stores,

    /// Vis produktkatalogen uten priser
    #[command(name = "produkter")]
    Products(ProductsArgs),

    /// Prisvarsler
    #[command(name = "overvak", subcommand)]
    Watch(WatchCommand),

    /// Planlagt daglig henting
    #[command(name = "planlegg", subcommand)]
    Schedule(ScheduleCommand),

    /// Eksporter prishistorikk
    #[command(name = "eksporter")]
    Export(ExportArgs),

    /// Vis eller endre konfigurasjonen
    #[command(name = "konfig", subcommand)]
    Config(ConfigCommand),

    /// Skriv ut skallfullføring
    #[command(name = "fullforing")]
    Completions {
        #[arg(value_enum, value_name = "SKALL")]
        shell: Shell,
    },
}

#[derive(Debug, Args)]
pub struct DealsArgs {
    #[command(flatten)]
    pub filters: SearchArgs,

    /// Vis også tilbud som ikke har startet ennå
    #[arg(long = "kommende")]
    pub upcoming: bool,
}

#[derive(Debug, Args)]
pub struct HistoryArgs {
    /// Produktet, f.eks. «monster ultra white»
    #[arg(value_name = "PRODUKT", required = true)]
    pub product: Vec<String>,

    /// Bare disse kjedene
    #[arg(
        long = "kjede",
        value_name = "KJEDE",
        value_enum,
        value_delimiter = ','
    )]
    pub chains: Vec<ChainChoice>,

    /// Antall dager bakover
    #[arg(long = "dager", value_name = "DAGER", default_value_t = 90)]
    pub days: u32,
}

#[derive(Debug, Args)]
pub struct UpdateArgs {
    /// Bare disse kildene
    #[arg(
        long = "kilde",
        value_name = "KILDE",
        value_enum,
        value_delimiter = ','
    )]
    pub sources: Vec<SourceId>,

    /// Ingen utdata utenom feil (for planlagte kjøringer)
    #[arg(long = "stille")]
    pub quiet: bool,
}

#[derive(Debug, Args)]
pub struct ProductsArgs {
    #[command(flatten)]
    pub filters: SearchArgs,

    /// Vis oppføringer fra kildene som ikke matchet noe katalogprodukt
    #[arg(long = "ukjente")]
    pub unknown: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum ExportFormat {
    #[default]
    #[value(name = "csv")]
    Csv,
}

#[derive(Debug, Args)]
pub struct ExportArgs {
    #[command(flatten)]
    pub filters: SearchArgs,

    /// Format
    #[arg(long = "format", value_enum, value_name = "FORMAT", default_value_t = ExportFormat::Csv)]
    pub format: ExportFormat,

    /// Fra og med dato (ÅÅÅÅ-MM-DD)
    #[arg(long = "fra", value_name = "DATO")]
    pub from: Option<Date>,

    /// Til og med dato (ÅÅÅÅ-MM-DD)
    #[arg(long = "til", value_name = "DATO")]
    pub to: Option<Date>,

    /// Excel-vennlig CSV: semikolon og desimalkomma
    #[arg(long = "excel")]
    pub excel: bool,
}

#[derive(Debug, Subcommand)]
pub enum WatchCommand {
    /// Legg til et prisvarsel
    #[command(name = "legg-til")]
    Add(AddAlertArgs),
    /// Vis prisvarsler
    #[command(name = "liste")]
    List,
    /// Fjern et prisvarsel
    #[command(name = "fjern")]
    Remove {
        #[arg(value_name = "ID")]
        id: i64,
    },
}

#[derive(Debug, Args)]
pub struct AddAlertArgs {
    /// Produktet, f.eks. «monster ultra white»
    #[arg(value_name = "PRODUKT", required = true)]
    pub product: Vec<String>,

    /// Varsle når prisen er under dette beløpet, i kr
    #[arg(long = "under", value_name = "KR", value_parser = parse_kr)]
    pub below: Ore,

    /// Bare denne kjeden
    #[arg(long = "kjede", value_enum, value_name = "KJEDE")]
    pub chain: Option<Chain>,

    /// Grensen gjelder literpris, ikke pris per beholder
    #[arg(long = "literpris")]
    pub liter_price: bool,
}

#[derive(Debug, Subcommand)]
pub enum ScheduleCommand {
    /// Installer daglig henting
    #[command(name = "installer")]
    Install {
        /// Klokkeslett for hentingen
        #[arg(long = "tid", default_value = "07:00", value_name = "TT:MM")]
        time: Time,
    },
    /// Fjern planlagt henting
    #[command(name = "fjern")]
    Remove,
    /// Vis status for planlagt henting
    #[command(name = "status")]
    Status,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Vis gjeldende konfigurasjon
    #[command(name = "vis")]
    Show,
    /// Vis hvor konfigurasjon, database og katalog ligger
    #[command(name = "sti")]
    Path,
    /// Sett en verdi, f.eks. `henting.ttl_timer 12`
    #[command(name = "sett")]
    Set {
        #[arg(value_name = "NØKKEL")]
        key: String,
        #[arg(value_name = "VERDI")]
        value: String,
    },
    /// Skriv en kommentert standardkonfigurasjon
    #[command(name = "init")]
    Init {
        /// Overskriv en eksisterende fil
        #[arg(long = "tving")]
        force: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coop_expands_to_all_coop_chains() {
        let chains = expand_chains(&[ChainChoice::AllCoop, ChainChoice::One(Chain::CoopObs)]);
        assert_eq!(chains, Chain::COOP);
    }

    #[test]
    fn clock_time_parses() {
        let time: Time = "07:00".parse().unwrap();
        assert_eq!((time.hour(), time.minute()), (7, 0));
    }
}
