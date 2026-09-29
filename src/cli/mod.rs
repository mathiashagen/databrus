//! The command line (SPEC §3).

pub mod args;
pub mod completions;

use std::ffi::OsString;

use clap::error::ErrorKind;
use clap::parser::ValueSource;
use clap::{Args, Command as ClapCommand, CommandFactory, FromArgMatches};

pub use args::*;

/// Help template with a Norwegian "Bruk:" instead of clap's "Usage:".
const HELP_TEMPLATE: &str = "{about-with-newline}\nBruk: {usage}\n\n{all-args}{after-help}";

/// The customized clap command. Used both for parsing and for shell completions.
pub fn command() -> ClapCommand {
    customize(Cli::command())
}

fn customize(command: ClapCommand) -> ClapCommand {
    command
        .help_template(HELP_TEMPLATE)
        // We define our own Norwegian, global `--help`.
        .disable_help_flag(true)
        .mut_subcommands(customize)
}

pub fn parse() -> Result<Cli, clap::Error> {
    parse_from(std::env::args_os())
}

pub fn parse_from<I, T>(args: I) -> Result<Cli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let mut command = command();
    let matches = command.try_get_matches_from_mut(args)?;

    // Top-level search filters only apply to the search without a command. Otherwise
    // `databrus --butikk kiwi tilbud` would silently ignore the filter.
    if let Some((name, _)) = matches.subcommand() {
        let filters = SearchArgs::augment_args(ClapCommand::new("filters"));
        let used = filters.get_arguments().find(|arg| {
            matches.value_source(arg.get_id().as_str()) == Some(ValueSource::CommandLine)
        });
        if let Some(arg) = used {
            let flag = arg
                .get_long()
                .map_or_else(|| arg.get_id().to_string(), |l| format!("--{l}"));
            return Err(command.error(
                ErrorKind::ArgumentConflict,
                format!("{flag} må stå etter kommandoen, f.eks. `databrus {name} {flag} …`"),
            ));
        }
    }

    Cli::from_arg_matches(&matches).map_err(|error| error.format(&mut command))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Chain;

    #[test]
    fn the_command_is_valid() {
        command().debug_assert();
    }

    #[test]
    fn free_text_without_command_is_a_search() {
        let cli = parse_from(["databrus", "monster", "ultra"]).unwrap();
        assert!(cli.command.is_none());
        assert_eq!(cli.search.query, ["monster", "ultra"]);
    }

    #[test]
    fn command_name_first_is_a_command() {
        let cli = parse_from(["databrus", "tilbud", "--butikk", "kiwi,coop"]).unwrap();
        let Some(Command::Deals(args)) = cli.command else {
            panic!("expected tilbud");
        };
        assert_eq!(
            args.filters.stores,
            [ChainChoice::One(Chain::Kiwi), ChainChoice::AllCoop]
        );
    }

    #[test]
    fn command_name_after_free_text_is_free_text() {
        let cli = parse_from(["databrus", "monster", "tilbud"]).unwrap();
        assert!(cli.command.is_none());
        assert_eq!(cli.search.query, ["monster", "tilbud"]);
    }

    #[test]
    fn global_options_work_before_and_after_the_command() {
        for args in [
            ["databrus", "--json", "butikker"],
            ["databrus", "butikker", "--json"],
        ] {
            let cli = parse_from(args).unwrap();
            assert!(cli.global.json, "{args:?}");
            assert!(
                matches!(cli.command, Some(Command::Stores)),
                "{args:?} gave {:?}",
                cli.command
            );
        }
    }

    #[test]
    fn filters_before_a_command_are_rejected() {
        let error = parse_from(["databrus", "--butikk", "kiwi", "tilbud"]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::ArgumentConflict);
        assert!(
            error
                .to_string()
                .contains("--butikk må stå etter kommandoen")
        );
        // After the command is fine.
        parse_from(["databrus", "tilbud", "--butikk", "kiwi"]).unwrap();
    }

    #[test]
    fn json_and_json_lines_are_exclusive() {
        assert!(parse_from(["databrus", "--json", "--json-linjer"]).is_err());
    }

    #[test]
    fn size_can_be_repeated() {
        let cli = parse_from(["databrus", "--storrelse", "0,33", "--storrelse", "500ml"]).unwrap();
        let ml: Vec<u32> = cli.search.sizes.iter().map(|m| m.get()).collect();
        assert_eq!(ml, [330, 500]);
    }

    /// Every subcommand and flag name is Norwegian, even though the Rust names are
    /// English. A missing `name =` or `long =` would leak an English name.
    #[test]
    fn all_command_and_flag_names_are_norwegian() {
        fn visit(command: &ClapCommand, names: &mut Vec<String>) {
            for arg in command.get_arguments() {
                if let Some(long) = arg.get_long() {
                    names.push(format!("--{long}"));
                }
            }
            for sub in command.get_subcommands() {
                names.push(sub.get_name().to_owned());
                visit(sub, names);
            }
        }
        let mut names = Vec::new();
        visit(&command(), &mut names);
        let english = [
            "search",
            "deals",
            "history",
            "update",
            "stores",
            "products",
            "watch",
            "schedule",
            "export",
            "config",
            "completions",
            "add",
            "list",
            "remove",
            "install",
            "show",
            "path",
            "set",
            "--refresh",
            "--offline",
            "--strict",
            "--color",
            "--verbose",
            "--config",
            "--brands",
            "--flavors",
            "--sizes",
            "--sugar-free",
            "--with-sugar",
            "--container",
            "--max-price",
            "--sort",
            "--single-unit",
            "--all",
            "--upcoming",
            "--product",
            "--chains",
            "--days",
            "--sources",
            "--quiet",
            "--unknown",
            "--from",
            "--to",
            "--below",
            "--chain",
            "--liter-price",
            "--time",
            "--force",
        ];
        for name in &names {
            assert!(
                !english.contains(&name.as_str()),
                "English name leaked: {name}"
            );
        }
        assert!(names.iter().any(|n| n == "legg-til"));
        assert!(names.iter().any(|n| n == "--maks-literpris"));
    }
}
