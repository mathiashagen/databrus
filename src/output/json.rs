//! The JSON contract (SPEC §9): a versioned envelope around each command's content. The
//! keys are Norwegian.

use std::io::{self, Write};

use jiff::Timestamp;
use schemars::JsonSchema;
use serde::Serialize;

use crate::error::AppError;
use crate::model;
use crate::sources::SourceStatus;

/// Bumped on breaking changes. New fields are not breaking.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Header {
    #[serde(rename = "skjemaversjon")]
    pub schema_version: u32,
    #[serde(rename = "generert")]
    pub generated: Timestamp,
    #[serde(rename = "kilder", skip_serializing_if = "Option::is_none")]
    pub sources: Option<Vec<SourceStatus>>,
}

impl Header {
    pub fn new() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            generated: model::now(),
            sources: None,
        }
    }

    pub fn with_sources(mut self, sources: Vec<SourceStatus>) -> Self {
        self.sources = Some(sources);
        self
    }
}

impl Default for Header {
    fn default() -> Self {
        Self::new()
    }
}

/// `{"skjemaversjon": 1, "generert": ..., <content>}`
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Envelope<T> {
    #[serde(flatten)]
    pub header: Header,
    #[serde(flatten)]
    pub content: T,
}

impl<T: Serialize> Envelope<T> {
    pub fn new(content: T) -> Self {
        Self {
            header: Header::new(),
            content,
        }
    }
}

#[derive(Serialize)]
struct Line<'a, T> {
    #[serde(rename = "type")]
    kind: &'static str,
    #[serde(flatten)]
    content: &'a T,
}

/// Writes one JSON document to stdout.
pub fn write<T: Serialize>(value: &T) -> Result<(), AppError> {
    let mut out = io::stdout().lock();
    serde_json::to_writer_pretty(&mut out, value)?;
    writeln!(out)?;
    Ok(())
}

/// Writes NDJSON (SPEC §9.2): the header first, then one line per row.
pub fn write_lines<'a, T, I>(header: &Header, rows: I) -> Result<(), AppError>
where
    T: Serialize + 'a,
    I: IntoIterator<Item = &'a T>,
{
    let mut out = io::stdout().lock();
    serde_json::to_writer(
        &mut out,
        &Line {
            kind: "hode",
            content: header,
        },
    )?;
    writeln!(out)?;
    for row in rows {
        serde_json::to_writer(
            &mut out,
            &Line {
                kind: "resultat",
                content: row,
            },
        )?;
        writeln!(out)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    struct Content {
        butikker: Vec<u32>,
    }

    #[test]
    fn the_envelope_is_flat() {
        let value = serde_json::to_value(Envelope::new(Content {
            butikker: vec![1, 2],
        }))
        .unwrap();
        assert_eq!(value["skjemaversjon"], 1);
        assert_eq!(value["butikker"], serde_json::json!([1, 2]));
        assert!(value.get("kilder").is_none());
        // Whole seconds, no fractions.
        let generated = value["generert"].as_str().unwrap();
        assert!(!generated.contains('.'), "{generated}");
    }
}
