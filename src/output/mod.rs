//! Output: table, JSON and NDJSON (SPEC §9, §11), and Norwegian number formatting.

pub mod format;
pub mod json;
pub mod results;
pub mod schema;
pub mod table;

/// Whether stdout should have colors, as anstream decided from `--farge`, `NO_COLOR` and
/// whether stdout is a terminal.
pub fn colors_enabled() -> bool {
    matches!(
        anstream::AutoStream::choice(&std::io::stdout()),
        anstream::ColorChoice::Always | anstream::ColorChoice::AlwaysAnsi
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Table,
    Json,
    JsonLines,
}
