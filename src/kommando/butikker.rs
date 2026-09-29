//! `databrus butikker` – kjente kjeder, hvilke kilder som dekker dem og hvor ferske
//! dataene er.

use jiff::Timestamp;
use serde::Serialize;

use crate::Kontekst;
use crate::feil::{AppFeil, Utgangskode};
use crate::kilder;
use crate::modell::{self, KildeId, Kjede};
use crate::utdata::json::{self, Hode, Konvolutt};
use crate::utdata::{Utdataformat, format, tabell};

#[derive(Debug, Serialize)]
struct Butikk {
    kjede: Kjede,
    navn: &'static str,
    gruppe: Option<&'static str>,
    kilder: Vec<KildeId>,
    /// Siste vellykkede henting fra en kilde som dekker kjeden.
    sist_hentet: Option<Timestamp>,
    /// Når den nyeste prisen for kjeden ble observert hos kilden. Kan være mye eldre enn
    /// `sist_hentet` (SPEC §4.5).
    nyeste_pris: Option<Timestamp>,
}

#[derive(Debug, Serialize)]
struct Innhold {
    butikker: Vec<Butikk>,
}

pub fn kjor(ktx: &Kontekst) -> Result<Utgangskode, AppFeil> {
    let lager = ktx.apne_lager()?;
    let aktive = kilder::aktive(&ktx.konfig, &[]);
    let mut sist_hentet = Vec::new();
    for kilde in &aktive {
        sist_hentet.push((kilde.id(), lager.siste_vellykkede_henting(kilde.id())?));
    }
    let nyeste_pris = lager.nyeste_pris_per_kjede()?;

    let butikker: Vec<Butikk> = Kjede::ALLE
        .into_iter()
        .map(|kjede| {
            let dekker: Vec<_> = aktive
                .iter()
                .filter(|k| k.kjeder().contains(&kjede))
                .map(|k| k.id())
                .collect();
            let sist = sist_hentet
                .iter()
                .filter(|(id, _)| dekker.contains(id))
                .filter_map(|(_, t)| *t)
                .max();
            Butikk {
                kjede,
                navn: kjede.visningsnavn(),
                gruppe: kjede.gruppe(),
                kilder: dekker,
                sist_hentet: sist,
                nyeste_pris: nyeste_pris.get(&kjede).copied(),
            }
        })
        .collect();

    match ktx.utdataformat() {
        Utdataformat::Json => json::skriv(&Konvolutt::ny(Innhold { butikker }))?,
        Utdataformat::JsonLinjer => json::skriv_linjer(&Hode::ny(), &butikker)?,
        Utdataformat::Tabell => {
            let na = modell::na();
            let siden = |t: Option<Timestamp>, ingen: &str| {
                t.map_or_else(
                    || ingen.to_owned(),
                    |t| {
                        let timer = u32::try_from(na.duration_since(t).as_hours()).unwrap_or(0);
                        format!("{} siden", format::alder(timer))
                    },
                )
            };
            let mut t = tabell::ny(&[
                "Kjede",
                "Valg",
                "Gruppe",
                "Kilder",
                "Sist hentet",
                "Nyeste pris",
            ]);
            for b in &butikker {
                let kilder: Vec<_> = b.kilder.iter().map(|k| k.slug()).collect();
                t.add_row(vec![
                    b.navn.to_owned(),
                    b.kjede.slug().to_owned(),
                    b.gruppe.unwrap_or("–").to_owned(),
                    kilder.join(", "),
                    siden(b.sist_hentet, "aldri hentet"),
                    siden(b.nyeste_pris, "–"),
                ]);
            }
            tabell::skriv(&t)?;
        }
    }
    Ok(Utgangskode::Ok)
}
