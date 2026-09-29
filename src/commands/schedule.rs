//! `databrus planlegg …` – scheduled daily fetching.

use crate::Context;
use crate::cli::ScheduleCommand;
use crate::error::{AppError, ExitStatus};
use crate::schedule;

pub fn run(command: &ScheduleCommand, _ctx: &Context) -> Result<ExitStatus, AppError> {
    let scheduler = schedule::for_platform();
    match command {
        ScheduleCommand::Install { time } => {
            scheduler.install(*time)?;
            println!(
                "installerte {} kl. {}",
                schedule::TASK_NAME,
                time.strftime("%H:%M")
            );
        }
        ScheduleCommand::Remove => {
            scheduler.remove()?;
            println!("fjernet {}", schedule::TASK_NAME);
        }
        ScheduleCommand::Status => match scheduler.status()? {
            Some(description) => println!("{description}"),
            None => println!("ingen planlagt henting er installert"),
        },
    }
    Ok(ExitStatus::Ok)
}
