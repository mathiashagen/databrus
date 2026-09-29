//! Table output (SPEC §11): clean columns without borders.

use std::io::Write;

use comfy_table::{ContentArrangement, Table, presets};

use crate::error::AppError;

pub fn new(headers: &[&str]) -> Table {
    let mut table = Table::new();
    table
        .load_style(presets::NOTHING)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(headers.to_vec());
    table
}

/// The terminal width, or `None` when stdout is not a terminal.
pub fn terminal_width() -> Option<u16> {
    Table::new().width()
}

pub fn write(table: &Table) -> Result<(), AppError> {
    writeln!(anstream::stdout(), "{table}")?;
    Ok(())
}
