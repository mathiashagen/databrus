//! `databrus planlegg …` – scheduled daily fetching.

use jiff::Timestamp;
use owo_colors::OwoColorize;

use crate::Context;
use crate::cli::ScheduleCommand;
use crate::error::{AppError, ExitStatus};
use crate::output::format;
use crate::schedule::{self, Job, TASK_NAME};
use crate::sources;

pub fn run(command: &ScheduleCommand, ctx: &Context) -> Result<ExitStatus, AppError> {
    let scheduler = schedule::for_platform();
    match command {
        ScheduleCommand::Install { time } => {
            let job = Job::for_current_exe(*time, ctx.global.config.as_deref())?;
            scheduler.install(&job)?;
            println!(
                "installerte {TASK_NAME}: daglig kl. {}",
                time.strftime("%H:%M")
            );
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

/// Whether the executable lives in a Cargo `target` directory.
fn is_build_output(program: &std::path::Path) -> bool {
    program.components().any(|c| c.as_os_str() == "target")
}
