//! Utdata: tabell, JSON og NDJSON (SPEC §9, §11), og norsk tallformatering.

pub mod format;
pub mod json;
pub mod tabell;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Utdataformat {
    Tabell,
    Json,
    JsonLinjer,
}
