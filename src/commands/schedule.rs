//! `databrus planlegg …` – scheduled daily fetching.

use jiff::Timestamp;
use owo_colors::OwoColorize;

use crate::Context;
use crate::cli::ScheduleCommand;
use crate::config;
use crate::error::{AppError, ExitStatus};
use crate::output::format;
use crate::schedule::{self, Job, TASK_NAME};
use crate::sources;

pub fn run(command: &ScheduleCommand, ctx: &Context) -> Result<ExitStatus, AppError> {
    let scheduler = schedule::for_platform();
    match command {
        ScheduleCommand::Install { time } => {
            let log_file = ctx.paths.data_dir.join("planlagt.log");
            let job = Job::for_current_exe(*time, ctx.global.config.as_deref(), log_file)?;
            let notes = scheduler.install(&job)?;
            println!(
                "installerte {TASK_NAME}: daglig kl. {}",
                time.strftime("%H:%M")
            );
            for note in notes {
                println!("  {note}");
            }
            warn_about_environment(ctx);
            if is_build_output(&job.program) {
                anstream::eprintln!(
                    "{} oppgaven kjører {}, som forsvinner ved `cargo clean` – \
                     installer databrus og kjør `planlegg installer` på nytt",
                    "advarsel:".yellow(),
                    job.program.display()
                );
            }
        }
        ScheduleCommand::Remove => {
            if scheduler.remove()? {
                println!("fjernet {TASK_NAME}");
            } else {
                println!("ingen planlagt henting er installert");
            }
        }
        ScheduleCommand::Status => status(scheduler.as_ref(), ctx)?,
    }
    Ok(ExitStatus::Ok)
}

fn status(scheduler: &dyn schedule::Scheduler, ctx: &Context) -> Result<(), AppError> {
    match scheduler.status()? {
        Some(installed) => {
            let when = installed.time.map_or("ukjent tid".to_owned(), |t| {
                format!("kl. {}", t.strftime("%H:%M"))
            });
            println!("{TASK_NAME} er installert: daglig {when}");
            println!("kommando: {}", installed.command);
        }
        None => println!("ingen planlagt henting er installert"),
    }

    // The task itself only knows that it ran, so the result comes from the fetch log.
    let db = ctx.open_database()?;
    let now = Timestamp::now();
    println!("siste henting:");
    for source in sources::enabled(&ctx.config, &[]) {
        let name = source.id().display_name();
        match db.last_fetch(source.id())? {
            None => println!("  {name}: aldri hentet"),
            Some(log) => {
                let hours = now.duration_since(log.finished).as_hours().max(0);
                let ago = match u32::try_from(hours).unwrap_or(u32::MAX) {
                    0 => "nettopp".to_owned(),
                    hours => format!("for {} siden", format::age(hours)),
                };
                if log.ok {
                    let count = log.listing_count.unwrap_or(0);
                    anstream::println!("  {name}: {} {ago}, {count} oppføringer", "ok".green());
                } else {
                    let error = log.error_message.as_deref().unwrap_or("ukjent feil");
                    anstream::println!("  {name}: {} {ago}: {error}", "feilet".red());
                }
            }
        }
    }
    Ok(())
}

/// The scheduled job does not run in this shell, so settings that only live in its
/// environment may not reach it.
fn warn_about_environment(ctx: &Context) {
    let set = |name: &str| std::env::var_os(name).is_some_and(|v| !v.is_empty());
    let kassalapp = &ctx.config.sources.kassalapp;
    if set(config::ENV_API_KEY) && kassalapp.enabled && kassalapp.api_key.is_empty() {
        anstream::eprintln!(
            "{} Kassalapp-nøkkelen er bare satt i miljøvariabelen {}, som den planlagte              hentingen kanskje ikke ser – lagre den med              `databrus konfig sett kilder.kassalapp.api_nokkel <NØKKEL>`",
            "advarsel:".yellow(),
            config::ENV_API_KEY
        );
    }
    if set(config::ENV_DATA_DIR) {
        anstream::eprintln!(
            "{} {} er satt i dette skallet; den planlagte hentingen bruker kanskje              standardmappen i stedet",
            "advarsel:".yellow(),
            config::ENV_DATA_DIR
        );
    }
}

/// Whether the executable lives in a Cargo `target` directory.
fn is_build_output(program: &std::path::Path) -> bool {
    program.components().any(|c| c.as_os_str() == "target")
}
