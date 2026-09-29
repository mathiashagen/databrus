//! databrus – find the cheapest energy drinks in Norway.
//!
//! All logic lives in the library; `main.rs` only parses arguments and turns the result
//! into an exit code. See `SPEC.md` for the specification.
//!
//! Source code is English. Everything the user sees – commands, flags, help text,
//! messages, JSON keys and config keys – is Norwegian (SPEC §2).

pub mod alerts;
pub mod catalog;
pub mod cli;
pub mod commands;
pub mod config;
pub mod db;
pub mod error;
pub mod history;
pub mod model;
pub mod output;
pub mod pricing;
pub mod schedule;
pub mod search;
pub mod sources;
#[cfg(test)]
mod test_support;

use catalog::Catalog;
use cli::{Cli, Command, GlobalArgs};
use config::{ColorMode, Config, Paths};
use db::Database;
use error::{AppError, ExitStatus};
use output::OutputFormat;
use sources::FetchMode;

/// Everything a command needs to run.
#[derive(Debug)]
pub struct Context {
    pub global: GlobalArgs,
    pub paths: Paths,
    pub config: Config,
}

impl Context {
    pub fn output_format(&self) -> OutputFormat {
        if self.global.json {
            OutputFormat::Json
        } else if self.global.json_lines {
            OutputFormat::JsonLines
        } else {
            OutputFormat::Table
        }
    }

    pub fn fetch_mode(&self) -> FetchMode {
        FetchMode {
            force: self.global.refresh,
            offline: self.global.offline,
            quiet: false,
        }
    }

    pub fn open_database(&self) -> Result<Database, AppError> {
        Database::open(&self.paths.database())
    }

    pub fn catalog(&self) -> Result<Catalog, AppError> {
        Catalog::load(&self.paths.catalog_override())
    }
}

/// Runs a parsed command line.
pub async fn run(cli: Cli) -> Result<ExitStatus, AppError> {
    let Cli {
        global,
        search,
        command,
    } = cli;
    let paths = Paths::find(global.config.as_deref())?;

    // These must work even when the config file is invalid.
    let command = match command {
        Some(Command::Completions { shell }) => {
            cli::completions::write(shell)?;
            return Ok(ExitStatus::Ok);
        }
        Some(Command::Config(c)) => return commands::config::run(&c, &paths, &global),
        other => other,
    };

    let config = Config::load(&paths.config_file)?;
    set_color(global.color.unwrap_or(config.color));
    let ctx = Context {
        global,
        paths,
        config,
    };

    match command {
        None => commands::search::run(&search, &ctx).await,
        Some(Command::Search(args)) => commands::search::run(&args, &ctx).await,
        Some(Command::Deals(args)) => commands::deals::run(&args, &ctx).await,
        Some(Command::History(args)) => commands::history::run(&args, &ctx),
        Some(Command::Update(args)) => commands::update::run(&args, &ctx).await,
        Some(Command::Stores) => commands::stores::run(&ctx),
        Some(Command::Products(args)) => commands::products::run(&args, &ctx),
        Some(Command::Watch(c)) => commands::watch::run(&c, &ctx),
        Some(Command::Schedule(c)) => commands::schedule::run(&c, &ctx),
        Some(Command::Export(args)) => commands::export::run(&args, &ctx),
        // Handled before the config was loaded.
        Some(Command::Completions { .. } | Command::Config(_)) => Ok(ExitStatus::Ok),
    }
}

fn set_color(mode: ColorMode) {
    let choice = match mode {
        ColorMode::Auto => anstream::ColorChoice::Auto,
        ColorMode::Always => anstream::ColorChoice::Always,
        ColorMode::Never => anstream::ColorChoice::Never,
    };
    choice.write_global();
}
