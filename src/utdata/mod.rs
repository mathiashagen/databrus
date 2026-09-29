//! Utdata: tabell, JSON og NDJSON (SPEC §9, §11), og norsk tallformatering.

pub mod format;
pub mod json;
pub mod resultater;
pub mod tabell;

/// Om stdout skal ha farger, slik anstream har avgjort det ut fra `--farge`,
/// `NO_COLOR` og om stdout er en terminal.
pub fn farger_pa() -> bool {
    matches!(
        anstream::AutoStream::choice(&std::io::stdout()),
        anstream::ColorChoice::Always | anstream::ColorChoice::AlwaysAnsi
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Utdataformat {
    Tabell,
    Json,
    JsonLinjer,
}
