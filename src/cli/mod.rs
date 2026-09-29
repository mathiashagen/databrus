//! Kommandolinjen (SPEC §3).

pub mod args;
pub mod fullforing;

use std::ffi::OsString;

use clap::error::ErrorKind;
use clap::parser::ValueSource;
use clap::{Args, Command, CommandFactory, FromArgMatches};

pub use args::*;

/// Hjelpemal med norsk «Bruk:» i stedet for clap sitt «Usage:».
const HJELPEMAL: &str = "{about-with-newline}\nBruk: {usage}\n\n{all-args}{after-help}";

/// Den ferdig tilpassede clap-kommandoen. Brukes både til tolking og til skallfullføring.
pub fn kommando() -> Command {
    tilpass(Cli::command())
}

fn tilpass(kommando: Command) -> Command {
    kommando
        .help_template(HJELPEMAL)
        // Vi definerer vårt eget, norske og globale `--help`.
        .disable_help_flag(true)
        .mut_subcommands(tilpass)
}

pub fn tolk() -> Result<Cli, clap::Error> {
    tolk_fra(std::env::args_os())
}

pub fn tolk_fra<I, T>(argumenter: I) -> Result<Cli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let mut kommando = kommando();
    let treff = kommando.try_get_matches_from_mut(argumenter)?;

    // Søkefiltre på toppnivå gjelder bare søket uten kommando. Ellers ville
    // `databrus --butikk kiwi tilbud` stille ignorert filteret.
    if let Some((navn, _)) = treff.subcommand() {
        let filtre = SokArgs::augment_args(Command::new("filtre"));
        let brukt = filtre.get_arguments().find(|arg| {
            treff.value_source(arg.get_id().as_str()) == Some(ValueSource::CommandLine)
        });
        if let Some(arg) = brukt {
            let flagg = arg
                .get_long()
                .map_or_else(|| arg.get_id().to_string(), |l| format!("--{l}"));
            return Err(kommando.error(
                ErrorKind::ArgumentConflict,
                format!("{flagg} må stå etter kommandoen, f.eks. `databrus {navn} {flagg} …`"),
            ));
        }
    }

    Cli::from_arg_matches(&treff).map_err(|feil| feil.format(&mut kommando))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kommandoen_er_gyldig() {
        kommando().debug_assert();
    }

    #[test]
    fn fritekst_uten_kommando_er_sok() {
        let cli = tolk_fra(["databrus", "monster", "ultra"]).unwrap();
        assert!(cli.kommando.is_none());
        assert_eq!(cli.sok.sok, ["monster", "ultra"]);
    }

    #[test]
    fn kommandonavn_forst_er_kommando() {
        let cli = tolk_fra(["databrus", "tilbud", "--butikk", "kiwi,coop"]).unwrap();
        let Some(Kommando::Tilbud(args)) = cli.kommando else {
            panic!("forventet tilbud");
        };
        assert_eq!(
            args.filtre.butikk,
            [
                KjedeValg::En(crate::modell::Kjede::Kiwi),
                KjedeValg::AlleCoop
            ]
        );
    }

    #[test]
    fn kommandonavn_etter_fritekst_er_fritekst() {
        let cli = tolk_fra(["databrus", "monster", "tilbud"]).unwrap();
        assert!(cli.kommando.is_none());
        assert_eq!(cli.sok.sok, ["monster", "tilbud"]);
    }

    #[test]
    fn globale_valg_virker_for_og_etter_kommando() {
        for argumenter in [
            ["databrus", "--json", "butikker"],
            ["databrus", "butikker", "--json"],
        ] {
            let cli = tolk_fra(argumenter).unwrap();
            assert!(cli.globale.json, "{argumenter:?}");
            assert!(
                matches!(cli.kommando, Some(Kommando::Butikker)),
                "{argumenter:?} ga {:?}",
                cli.kommando
            );
        }
    }

    #[test]
    fn filtre_foran_kommando_avvises() {
        let feil = tolk_fra(["databrus", "--butikk", "kiwi", "tilbud"]).unwrap_err();
        assert_eq!(feil.kind(), ErrorKind::ArgumentConflict);
        assert!(
            feil.to_string()
                .contains("--butikk må stå etter kommandoen")
        );
        // Etter kommandoen er det i orden.
        tolk_fra(["databrus", "tilbud", "--butikk", "kiwi"]).unwrap();
    }

    #[test]
    fn json_og_json_linjer_utelukker_hverandre() {
        assert!(tolk_fra(["databrus", "--json", "--json-linjer"]).is_err());
    }

    #[test]
    fn storrelse_kan_gjentas() {
        let cli = tolk_fra(["databrus", "--storrelse", "0,33", "--storrelse", "500ml"]).unwrap();
        let ml: Vec<u32> = cli.sok.storrelse.iter().map(|m| m.get()).collect();
        assert_eq!(ml, [330, 500]);
    }
}
