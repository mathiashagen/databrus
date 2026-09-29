//! Clap-definisjonene. Bare strukturer her – logikken ligger i [`crate::kommando`].
//!
//! Kommando- og flaggnavn er ASCII (æ→ae, ø→o, å→a), mens hjelpetekst er vanlig norsk (SPEC §2).

use std::path::PathBuf;

use clap::builder::PossibleValue;
use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;
use jiff::civil::{Date, Time};

use crate::katalog::tolk::tolk_storrelse;
use crate::konfig::Fargevalg;
use crate::modell::{Beholder, KildeId, Kjede, Ml, Ore};
use crate::sok::Sortering;
use crate::utdata::format::tolk_kr;

// Uten kommando er fritekst og filtre et søk: `databrus monster ultra --storrelse 0,5`.
// Et kommandonavn teller bare som kommando når ingen fritekst står foran; `databrus monster
// tilbud` søker etter «monster tilbud», og `databrus sok tilbud` søker etter «tilbud».
// Filtre foran en kommando avvises i `tolk_fra` (de hører hjemme etter kommandoen).
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
    pub globale: Globale,

    #[command(flatten)]
    pub sok: SokArgs,

    #[command(subcommand)]
    pub kommando: Option<Kommando>,
}

/// Valg som gjelder alle kommandoer (SPEC §3.3).
#[derive(Debug, Clone, Args)]
#[command(next_help_heading = "Globale valg")]
pub struct Globale {
    /// Skriv ut et versjonert JSON-dokument
    #[arg(long, global = true, conflicts_with = "json_linjer")]
    pub json: bool,

    /// Skriv ut NDJSON: én hodelinje, deretter én linje per rad
    #[arg(long = "json-linjer", global = true)]
    pub json_linjer: bool,

    /// Hent ferske priser før svar (tidligst 15 min etter forrige henting)
    #[arg(long, global = true, conflicts_with = "frakoblet")]
    pub oppdater: bool,

    /// Bruk bare lokale data – aldri nettverket
    #[arg(long, global = true)]
    pub frakoblet: bool,

    /// Avslutt med kode 2 hvis en kilde feilet eller dataene er utdaterte
    #[arg(long, global = true)]
    pub streng: bool,

    /// Farger i utdata
    #[arg(long, global = true, value_enum, value_name = "NÅR")]
    pub farge: Option<Fargevalg>,

    /// Mer diagnostikk på stderr (-v, -vv)
    #[arg(short = 'v', long = "detaljert", action = ArgAction::Count, global = true)]
    pub detaljert: u8,

    /// Bruk en annen konfigurasjonsfil
    #[arg(
        long,
        global = true,
        env = "DATABRUS_KONFIG",
        hide_env_values = true,
        value_name = "STI"
    )]
    pub konfig: Option<PathBuf>,

    /// Vis hjelp
    #[arg(short = 'h', long = "help", action = ArgAction::Help, global = true)]
    pub hjelp: Option<bool>,

    /// Vis versjon
    #[arg(short = 'V', long = "version", action = ArgAction::Version)]
    pub versjon: Option<bool>,
}

/// Søkefiltre, delt av søk, `tilbud`, `produkter` og `eksporter` (SPEC §3.2).
#[derive(Debug, Clone, Default, Args)]
#[command(next_help_heading = "Filtre")]
pub struct SokArgs {
    /// Fritekst, f.eks. «monster ultra»
    #[arg(value_name = "SØK")]
    pub sok: Vec<String>,

    /// Merke, f.eks. monster, red-bull, nocco (kan gjentas)
    #[arg(long, value_name = "MERKE")]
    pub merke: Vec<String>,

    /// Smak (kan gjentas)
    #[arg(long, value_name = "SMAK")]
    pub smak: Vec<String>,

    /// Størrelse: 0,5 · 500ml · 0,33l (kan gjentas, ikke kommaliste)
    #[arg(long, value_name = "STR", value_parser = tolk_storrelse)]
    pub storrelse: Vec<Ml>,

    /// Kjeder, f.eks. kiwi,rema – «coop» betyr alle Coop-kjedene
    #[arg(long, value_name = "KJEDE", value_enum, value_delimiter = ',')]
    pub butikk: Vec<KjedeValg>,

    /// Bare sukkerfrie
    #[arg(long, conflicts_with = "med_sukker")]
    pub sukkerfri: bool,

    /// Bare med sukker
    #[arg(long = "med-sukker")]
    pub med_sukker: bool,

    /// Beholdertype
    #[arg(long, value_enum)]
    pub beholder: Option<Beholder>,

    /// Høyeste effektive pris per beholder, i kr
    #[arg(long = "maks-pris", value_name = "KR", value_parser = tolk_kr)]
    pub maks_pris: Option<Ore>,

    /// Høyeste literpris, i kr
    #[arg(long = "maks-literpris", value_name = "KR", value_parser = tolk_kr)]
    pub maks_literpris: Option<Ore>,

    /// Sortering
    #[arg(long, value_enum, default_value_t = Sortering::Literpris)]
    pub sorter: Sortering,

    /// Ranger etter pris for én beholder og se bort fra flerkjøpstilbud
    #[arg(long)]
    pub enkeltvis: bool,

    /// Vis alle rader, også utsolgte og svært gamle
    #[arg(long)]
    pub alle: bool,
}

/// Et kjedevalg på kommandolinjen: én kjede, eller `coop` for alle Coop-kjedene.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KjedeValg {
    En(Kjede),
    AlleCoop,
}

const KJEDEVALG: [KjedeValg; 12] = [
    KjedeValg::En(Kjede::Rema),
    KjedeValg::En(Kjede::Kiwi),
    KjedeValg::En(Kjede::Meny),
    KjedeValg::En(Kjede::Spar),
    KjedeValg::En(Kjede::Joker),
    KjedeValg::AlleCoop,
    KjedeValg::En(Kjede::CoopExtra),
    KjedeValg::En(Kjede::CoopObs),
    KjedeValg::En(Kjede::CoopMega),
    KjedeValg::En(Kjede::CoopPrix),
    KjedeValg::En(Kjede::Bunnpris),
    KjedeValg::En(Kjede::Oda),
];

impl ValueEnum for KjedeValg {
    fn value_variants<'a>() -> &'a [Self] {
        &KJEDEVALG
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        Some(match self {
            KjedeValg::En(kjede) => PossibleValue::new(kjede.slug()),
            KjedeValg::AlleCoop => PossibleValue::new("coop").help("alle Coop-kjedene"),
        })
    }
}

/// Gjør kjedevalg om til en sortert liste uten duplikater.
pub fn utvid_kjeder(valg: &[KjedeValg]) -> Vec<Kjede> {
    let mut kjeder: Vec<Kjede> = valg
        .iter()
        .flat_map(|v| match v {
            KjedeValg::En(kjede) => vec![*kjede],
            KjedeValg::AlleCoop => Kjede::COOP.to_vec(),
        })
        .collect();
    kjeder.sort();
    kjeder.dedup();
    kjeder
}

#[derive(Debug, Subcommand)]
pub enum Kommando {
    /// Søk etter energidrikker (det samme som uten kommando)
    Sok(SokArgs),

    /// Vis aktuelle tilbud, sortert etter hvor gode de er
    Tilbud(TilbudArgs),

    /// Vis prishistorikk for et produkt
    Historikk(HistorikkArgs),

    /// Hent priser fra alle kilder nå
    Oppdater(OppdaterArgs),

    /// Vis kjeder, kilder og hvor ferske dataene er
    Butikker,

    /// Vis produktkatalogen uten priser
    Produkter(ProdukterArgs),

    /// Prisvarsler
    #[command(subcommand)]
    Overvak(OvervakKommando),

    /// Planlagt daglig henting
    #[command(subcommand)]
    Planlegg(PlanleggKommando),

    /// Eksporter prishistorikk
    Eksporter(EksporterArgs),

    /// Vis eller endre konfigurasjonen
    #[command(subcommand)]
    Konfig(KonfigKommando),

    /// Skriv ut skallfullføring
    Fullforing {
        #[arg(value_enum, value_name = "SKALL")]
        skall: Shell,
    },
}

#[derive(Debug, Args)]
pub struct TilbudArgs {
    #[command(flatten)]
    pub filtre: SokArgs,

    /// Vis også tilbud som ikke har startet ennå
    #[arg(long)]
    pub kommende: bool,
}

#[derive(Debug, Args)]
pub struct HistorikkArgs {
    /// Produktet, f.eks. «monster ultra white»
    #[arg(value_name = "PRODUKT", required = true)]
    pub produkt: Vec<String>,

    /// Bare disse kjedene
    #[arg(long, value_name = "KJEDE", value_enum, value_delimiter = ',')]
    pub kjede: Vec<KjedeValg>,

    /// Antall dager bakover
    #[arg(long, default_value_t = 90)]
    pub dager: u32,
}

#[derive(Debug, Args)]
pub struct OppdaterArgs {
    /// Bare disse kildene
    #[arg(long, value_name = "KILDE", value_enum, value_delimiter = ',')]
    pub kilde: Vec<KildeId>,

    /// Ingen utdata utenom feil (for planlagte kjøringer)
    #[arg(long)]
    pub stille: bool,
}

#[derive(Debug, Args)]
pub struct ProdukterArgs {
    #[command(flatten)]
    pub filtre: SokArgs,

    /// Vis oppføringer fra kildene som ikke matchet noe katalogprodukt
    #[arg(long)]
    pub ukjente: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum Eksportformat {
    #[default]
    Csv,
}

#[derive(Debug, Args)]
pub struct EksporterArgs {
    #[command(flatten)]
    pub filtre: SokArgs,

    /// Format
    #[arg(long, value_enum, default_value_t = Eksportformat::Csv)]
    pub format: Eksportformat,

    /// Fra og med dato (ÅÅÅÅ-MM-DD)
    #[arg(long, value_name = "DATO")]
    pub fra: Option<Date>,

    /// Til og med dato (ÅÅÅÅ-MM-DD)
    #[arg(long, value_name = "DATO")]
    pub til: Option<Date>,

    /// Excel-vennlig CSV: semikolon og desimalkomma
    #[arg(long)]
    pub excel: bool,
}

#[derive(Debug, Subcommand)]
pub enum OvervakKommando {
    /// Legg til et prisvarsel
    LeggTil(LeggTilArgs),
    /// Vis prisvarsler
    Liste,
    /// Fjern et prisvarsel
    Fjern {
        #[arg(value_name = "ID")]
        id: i64,
    },
}

#[derive(Debug, Args)]
pub struct LeggTilArgs {
    /// Produktet, f.eks. «monster ultra white»
    #[arg(value_name = "PRODUKT", required = true)]
    pub produkt: Vec<String>,

    /// Varsle når prisen er under dette beløpet, i kr
    #[arg(long, value_name = "KR", value_parser = tolk_kr)]
    pub under: Ore,

    /// Bare denne kjeden
    #[arg(long, value_enum)]
    pub kjede: Option<Kjede>,

    /// Grensen gjelder literpris, ikke pris per beholder
    #[arg(long)]
    pub literpris: bool,
}

#[derive(Debug, Subcommand)]
pub enum PlanleggKommando {
    /// Installer daglig henting
    Installer {
        /// Klokkeslett for hentingen
        #[arg(long, default_value = "07:00", value_name = "TT:MM")]
        tid: Time,
    },
    /// Fjern planlagt henting
    Fjern,
    /// Vis status for planlagt henting
    Status,
}

#[derive(Debug, Subcommand)]
pub enum KonfigKommando {
    /// Vis gjeldende konfigurasjon
    Vis,
    /// Vis hvor konfigurasjon, database og katalog ligger
    Sti,
    /// Sett en verdi, f.eks. `henting.ttl_timer 12`
    Sett {
        #[arg(value_name = "NØKKEL")]
        nokkel: String,
        #[arg(value_name = "VERDI")]
        verdi: String,
    },
    /// Skriv en kommentert standardkonfigurasjon
    Init {
        /// Overskriv en eksisterende fil
        #[arg(long)]
        tving: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coop_utvides_til_alle_coop_kjedene() {
        let kjeder = utvid_kjeder(&[KjedeValg::AlleCoop, KjedeValg::En(Kjede::CoopObs)]);
        assert_eq!(kjeder, Kjede::COOP);
    }

    #[test]
    fn klokkeslett_tolkes() {
        let tid: Time = "07:00".parse().unwrap();
        assert_eq!((tid.hour(), tid.minute()), (7, 0));
    }
}
