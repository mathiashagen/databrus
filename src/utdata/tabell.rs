//! Tabellutdata (SPEC §11): rene kolonner uten rammer.

use std::io::Write;

use comfy_table::{ContentArrangement, Table, presets};

use crate::feil::AppFeil;

pub fn ny(overskrifter: &[&str]) -> Table {
    let mut tabell = Table::new();
    tabell
        .load_style(presets::NOTHING)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(overskrifter.to_vec());
    tabell
}

/// Terminalbredden, eller `None` når stdout ikke er en terminal.
pub fn terminalbredde() -> Option<u16> {
    Table::new().width()
}

pub fn skriv(tabell: &Table) -> Result<(), AppFeil> {
    writeln!(anstream::stdout(), "{tabell}")?;
    Ok(())
}
