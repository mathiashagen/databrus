use std::process::ExitCode;

use clap::error::ErrorKind;
use databrus::feil::Utgangskode;
use owo_colors::OwoColorize;
use tracing_subscriber::EnvFilter;

fn main() -> ExitCode {
    let cli = match databrus::cli::tolk() {
        Ok(cli) => cli,
        Err(feil) => {
            let _ = feil.print();
            // Brukerfeil gir kode 1, ikke clap sin standard 2 (som er reservert for --streng).
            let kode = match feil.kind() {
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => Utgangskode::Ok,
                _ => Utgangskode::Feil,
            };
            return kode.into();
        }
    };

    start_logging(cli.globale.detaljert);

    let kjoretid = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(kjoretid) => kjoretid,
        Err(feil) => {
            anstream::eprintln!(
                "{} kunne ikke starte kjøretiden: {feil}",
                "feil:".red().bold()
            );
            return Utgangskode::Feil.into();
        }
    };

    match kjoretid.block_on(databrus::kjor(cli)) {
        Ok(kode) => kode.into(),
        Err(feil) => {
            anstream::eprintln!("{} {feil}", "feil:".red().bold());
            feil.utgangskode().into()
        }
    }
}

/// Diagnostikk går alltid til stderr, slik at stdout forblir ren tabell eller JSON.
fn start_logging(detaljert: u8) {
    let standard = match detaljert {
        0 => "warn",
        1 => "databrus=debug",
        _ => "debug",
    };
    let filter =
        EnvFilter::try_from_env("DATABRUS_LOGG").unwrap_or_else(|_| EnvFilter::new(standard));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();
}
