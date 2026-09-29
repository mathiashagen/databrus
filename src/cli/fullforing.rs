//! Skallfullføring (`databrus fullforing <skall>`).

use std::io;

use clap_complete::Shell;

use crate::feil::AppFeil;

pub fn skriv(skall: Shell) -> Result<(), AppFeil> {
    let mut kommando = super::kommando();
    clap_complete::generate(skall, &mut kommando, "databrus", &mut io::stdout().lock());
    Ok(())
}
