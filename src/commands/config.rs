//! `databrus konfig …` – runs before the config is loaded, so `init` and `sett` work even
//! when the file is invalid.

use std::fs;

use serde::Serialize;

use crate::cli::{ConfigCommand, GlobalArgs};
use crate::config::{self, Config, DEFAULT_TOML, Paths};
use crate::error::{AppError, ExitStatus};
use crate::output::json::{self, Envelope};

#[derive(Serialize)]
struct ConfigContent {
    #[serde(rename = "konfig")]
    config: Config,
}

#[derive(Serialize)]
struct PathsContent {
    #[serde(rename = "konfig")]
    config: String,
    database: String,
    #[serde(rename = "katalog")]
    catalog: String,
}

pub fn run(
    command: &ConfigCommand,
    paths: &Paths,
    global: &GlobalArgs,
) -> Result<ExitStatus, AppError> {
    match command {
        ConfigCommand::Show => {
            let config = Config::load(&paths.config_file)?.redacted();
            if !paths.config_file.exists() {
                eprintln!(
                    "(ingen konfigurasjonsfil på {} – viser standardverdiene)",
                    paths.config_file.display()
                );
            }
            if global.json {
                json::write(&Envelope::new(ConfigContent { config }))?;
            } else {
                let text = toml::to_string_pretty(&config)
                    .map_err(|error| AppError::Config(error.to_string()))?;
                print!("{text}");
            }
        }
        ConfigCommand::Path => {
            let content = PathsContent {
                config: paths.config_file.display().to_string(),
                database: paths.database().display().to_string(),
                catalog: paths.catalog_override().display().to_string(),
            };
            if global.json {
                json::write(&Envelope::new(content))?;
            } else {
                println!("konfigurasjon  {}", content.config);
                println!("database       {}", content.database);
                println!("katalog        {}", content.catalog);
            }
        }
        ConfigCommand::Set { key, value } => {
            config::set_value(&paths.config_file, key, value)?;
            // The value is not printed – it may be an API key.
            eprintln!("satte {key} i {}", paths.config_file.display());
        }
        ConfigCommand::Init { force } => {
            if paths.config_file.exists() && !force {
                return Err(AppError::Usage(format!(
                    "{} finnes allerede – bruk --tving for å overskrive",
                    paths.config_file.display()
                )));
            }
            if let Some(dir) = paths.config_file.parent() {
                fs::create_dir_all(dir)?;
            }
            fs::write(&paths.config_file, DEFAULT_TOML)?;
            eprintln!("skrev {}", paths.config_file.display());
        }
    }
    Ok(ExitStatus::Ok)
}
