//! `databrus oppdater` – fetches from all (or selected) sources now.

use owo_colors::OwoColorize;

use crate::Context;
use crate::cli::UpdateArgs;
use crate::error::{AppError, ExitStatus};
use crate::output::OutputFormat;
use crate::output::json::{self, Envelope, Header};
use crate::sources::{self, FetchMode, SourceState};

pub async fn run(args: &UpdateArgs, ctx: &Context) -> Result<ExitStatus, AppError> {
    if ctx.global.offline {
        return Err(AppError::Usage("kan ikke oppdatere med --frakoblet".into()));
    }
    let mode = FetchMode {
        force: true,
        offline: false,
        quiet: args.quiet,
    };
    let catalog = ctx.catalog()?;
    let mut db = ctx.open_database()?;
    let statuses = sources::refresh(&mut db, &ctx.config, &catalog, mode, &args.sources).await?;

    let fetched: Vec<_> = statuses
        .iter()
        .filter(|s| s.new_listings.is_some())
        .collect();
    let failed = statuses
        .iter()
        .filter(|s| s.status == SourceState::Failed)
        .count();

    match ctx.output_format() {
        OutputFormat::Json => {
            json::write(&Envelope::new(Header::new().with_sources(statuses.clone())))?
        }
        OutputFormat::JsonLines => json::write_lines(&Header::new(), &statuses)?,
        OutputFormat::Table if !args.quiet => {
            for status in &fetched {
                anstream::println!(
                    "{} {}: {} oppføringer, {} matchet katalogen",
                    "✓".green(),
                    status.id.display_name(),
                    status.new_listings.unwrap_or(0),
                    status.matched.unwrap_or(0)
                );
            }
        }
        OutputFormat::Table => {}
    }
    super::check_alerts(&mut db, &catalog, ctx, &statuses)?;

    if fetched.is_empty() && failed > 0 {
        return Err(AppError::AllSourcesFailed);
    }
    if ctx.global.strict && failed > 0 {
        return Ok(ExitStatus::Strict);
    }
    Ok(ExitStatus::Ok)
}
