//! JSON-kontrakten (SPEC §9): en versjonert konvolutt rundt hver kommandos innhold.

use std::io::{self, Write};

use jiff::Timestamp;
use serde::Serialize;

use crate::feil::AppFeil;
use crate::kilder::KildeInfo;
use crate::modell;

/// Økes ved brytende endringer. Nye felt er ikke brytende.
pub const SKJEMAVERSJON: u32 = 1;

#[derive(Debug, Clone, Serialize)]
pub struct Hode {
    pub skjemaversjon: u32,
    pub generert: Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kilder: Option<Vec<KildeInfo>>,
}

impl Hode {
    pub fn ny() -> Self {
        Self {
            skjemaversjon: SKJEMAVERSJON,
            generert: modell::na(),
            kilder: None,
        }
    }

    pub fn med_kilder(mut self, kilder: Vec<KildeInfo>) -> Self {
        self.kilder = Some(kilder);
        self
    }
}

/// `{"skjemaversjon": 1, "generert": ..., <innhold>}`
#[derive(Debug, Clone, Serialize)]
pub struct Konvolutt<T> {
    #[serde(flatten)]
    pub hode: Hode,
    #[serde(flatten)]
    pub innhold: T,
}

impl<T: Serialize> Konvolutt<T> {
    pub fn ny(innhold: T) -> Self {
        Self {
            hode: Hode::ny(),
            innhold,
        }
    }
}

#[derive(Serialize)]
struct Linje<'a, T> {
    #[serde(rename = "type")]
    slag: &'static str,
    #[serde(flatten)]
    innhold: &'a T,
}

/// Skriver ett JSON-dokument til stdout.
pub fn skriv<T: Serialize>(verdi: &T) -> Result<(), AppFeil> {
    let mut ut = io::stdout().lock();
    serde_json::to_writer_pretty(&mut ut, verdi)?;
    writeln!(ut)?;
    Ok(())
}

/// Skriver NDJSON (SPEC §9.2): først hodet, så én linje per rad.
pub fn skriv_linjer<'a, T, I>(hode: &Hode, rader: I) -> Result<(), AppFeil>
where
    T: Serialize + 'a,
    I: IntoIterator<Item = &'a T>,
{
    let mut ut = io::stdout().lock();
    serde_json::to_writer(
        &mut ut,
        &Linje {
            slag: "hode",
            innhold: hode,
        },
    )?;
    writeln!(ut)?;
    for rad in rader {
        serde_json::to_writer(
            &mut ut,
            &Linje {
                slag: "resultat",
                innhold: rad,
            },
        )?;
        writeln!(ut)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    struct Innhold {
        butikker: Vec<u32>,
    }

    #[test]
    fn konvolutten_er_flat() {
        let verdi = serde_json::to_value(Konvolutt::ny(Innhold {
            butikker: vec![1, 2],
        }))
        .unwrap();
        assert_eq!(verdi["skjemaversjon"], 1);
        assert_eq!(verdi["butikker"], serde_json::json!([1, 2]));
        assert!(verdi.get("kilder").is_none());
        // Hele sekunder, uten brøkdeler.
        let generert = verdi["generert"].as_str().unwrap();
        assert!(!generert.contains('.'), "{generert}");
    }
}
