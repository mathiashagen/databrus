//! `databrus overvak …` – price alerts.

use crate::Context;
use crate::cli::WatchCommand;
use crate::error::{AppError, ExitStatus};

pub fn run(_command: &WatchCommand, _ctx: &Context) -> Result<ExitStatus, AppError> {
    Err(AppError::NotImplemented("prisvarsler, M4"))
}
