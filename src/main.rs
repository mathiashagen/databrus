use std::io;
use std::process::ExitCode;

use clap::error::ErrorKind;
use databrus::error::{AppError, ExitStatus};
use owo_colors::OwoColorize;
use tracing_subscriber::EnvFilter;

fn main() -> ExitCode {
    let cli = match databrus::cli::parse() {
        Ok(cli) => cli,
        Err(error) => {
            let _ = error.print();
            // Usage errors exit with 1, not clap's default 2 (reserved for --streng).
            let status = match error.kind() {
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => ExitStatus::Ok,
                _ => ExitStatus::Error,
            };
            return status.into();
        }
    };

    init_logging(cli.global.verbose);

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            anstream::eprintln!(
                "{} kunne ikke starte kjøretiden: {error}",
                "feil:".red().bold()
            );
            return ExitStatus::Error.into();
        }
    };

    match runtime.block_on(databrus::run(cli)) {
        Ok(status) => status.into(),
        // The reader closed the pipe (e.g. `databrus produkter | head`): perfectly normal.
        Err(AppError::Io(error)) if error.kind() == io::ErrorKind::BrokenPipe => {
            ExitStatus::Ok.into()
        }
        Err(error) => {
            anstream::eprintln!("{} {error}", "feil:".red().bold());
            error.exit_status().into()
        }
    }
}

/// Diagnostics always go to stderr, so stdout stays a clean table or JSON.
fn init_logging(verbose: u8) {
    let default = match verbose {
        0 => "warn",
        1 => "databrus=debug",
        _ => "debug",
    };
    let filter =
        EnvFilter::try_from_env("DATABRUS_LOGG").unwrap_or_else(|_| EnvFilter::new(default));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();
}
