//! Shell completions (`databrus fullforing <skall>`).

use std::io;

use clap_complete::Shell;

use crate::error::AppError;

pub fn write(shell: Shell) -> Result<(), AppError> {
    let mut command = super::command();
    clap_complete::generate(shell, &mut command, "databrus", &mut io::stdout().lock());
    Ok(())
}
