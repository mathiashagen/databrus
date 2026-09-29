//! Lesing. Spørringene for søk, historikk og eksport kommer i M1/M2.

use std::collections::HashMap;

use jiff::Timestamp;
use rusqlite::types::Type;
use rusqlite::{OptionalExtension, Row};
use serde::Serialize;

use super::{Lager, feil};
use crate::feil::AppFeil;
use crate::modell::{KildeId, Kjede};

/// Én rad fra hentingsloggen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hentelogg {
    pub kilde: KildeId,
    pub startet: Timestamp,
    pub fullfort: Timestamp,
    pub ok: bool,
    pub feilmelding: Option<String>,
    pub antall_oppforinger: Option<i64>,
}

/// En oppføring som ikke matchet noe katalogprodukt (`produkter --ukjente`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Umatchet {
    pub kilde: String,
    pub kjede: String,
    pub kilde_produkt_id: String,
    pub raanavn: String,
    pub gtin: Option<String>,
    pub sist_sett: Timestamp,
}

impl Lager {
    pub fn har_prisdata(&self) -> Result<bool, AppFeil> {
        self.conn
            .query_row("SELECT EXISTS (SELECT 1 FROM prisintervall)", [], |rad| {
                rad.get(0)
            })
            .map_err(|e| feil(&self.sti, e))
    }

    pub fn antall_prisintervaller(&self) -> Result<i64, AppFeil> {
        self.conn
            .query_row("SELECT COUNT(*) FROM prisintervall", [], |rad| rad.get(0))
            .map_err(|e| feil(&self.sti, e))
    }

    /// Når den nyeste prisen per kjede ble observert. Rader med ukjent kjedeslug ignoreres.
    pub fn nyeste_pris_per_kjede(&self) -> Result<HashMap<Kjede, Timestamp>, AppFeil> {
        let mut sporring = self
            .conn
            .prepare(
                "SELECT o.kjede, MAX(p.sist_sett)
                 FROM prisintervall p JOIN oppforing o ON o.id = p.oppforing_id
                 GROUP BY o.kjede",
            )
            .map_err(|e| feil(&self.sti, e))?;
        let rader = sporring
            .query_map([], |rad| Ok((rad.get::<_, String>(0)?, tidspunkt(rad, 1)?)))
            .map_err(|e| feil(&self.sti, e))?;
        let mut kart = HashMap::new();
        for rad in rader {
            let (slug, tid) = rad.map_err(|e| feil(&self.sti, e))?;
            if let Some(kjede) = Kjede::fra_slug(&slug) {
                kart.insert(kjede, tid);
            }
        }
        Ok(kart)
    }

    /// Når prisen til en oppføring sist ble observert.
    pub fn siste_pris_sett(&self, oppforing_id: i64) -> Result<Option<Timestamp>, AppFeil> {
        self.conn
            .query_row(
                "SELECT MAX(sist_sett) FROM prisintervall WHERE oppforing_id = ?1",
                [oppforing_id],
                |rad| rad.get::<_, Option<i64>>(0),
            )
            .map_err(|e| feil(&self.sti, e))?
            .map(|sek| Timestamp::from_second(sek).map_err(|e| feil(&self.sti, e)))
            .transpose()
    }

    /// Det siste henteforsøket for en kilde, vellykket eller ikke.
    pub fn siste_henting(&self, kilde: KildeId) -> Result<Option<Hentelogg>, AppFeil> {
        self.conn
            .query_row(
                "SELECT startet, fullfort, status, feilmelding, antall_oppforinger
                 FROM henting WHERE kilde = ?1
                 ORDER BY startet DESC, id DESC LIMIT 1",
                [kilde.slug()],
                |rad| {
                    Ok(Hentelogg {
                        kilde,
                        startet: tidspunkt(rad, 0)?,
                        fullfort: tidspunkt(rad, 1)?,
                        ok: rad.get::<_, String>(2)? == "ok",
                        feilmelding: rad.get(3)?,
                        antall_oppforinger: rad.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(|e| feil(&self.sti, e))
    }

    pub fn siste_vellykkede_henting(&self, kilde: KildeId) -> Result<Option<Timestamp>, AppFeil> {
        self.conn
            .query_row(
                "SELECT fullfort FROM henting WHERE kilde = ?1 AND status = 'ok'
                 ORDER BY fullfort DESC LIMIT 1",
                [kilde.slug()],
                |rad| tidspunkt(rad, 0),
            )
            .optional()
            .map_err(|e| feil(&self.sti, e))
    }

    pub fn umatchede_oppforinger(&self) -> Result<Vec<Umatchet>, AppFeil> {
        let mut sporring = self
            .conn
            .prepare(
                "SELECT kilde, kjede, kilde_produkt_id, raanavn, gtin, sist_sett
                 FROM oppforing WHERE produkt_id IS NULL
                 ORDER BY raanavn, kjede",
            )
            .map_err(|e| feil(&self.sti, e))?;
        let rader = sporring
            .query_map([], |rad| {
                Ok(Umatchet {
                    kilde: rad.get(0)?,
                    kjede: rad.get(1)?,
                    kilde_produkt_id: rad.get(2)?,
                    raanavn: rad.get(3)?,
                    gtin: rad.get(4)?,
                    sist_sett: tidspunkt(rad, 5)?,
                })
            })
            .map_err(|e| feil(&self.sti, e))?;
        rader
            .collect::<Result<_, _>>()
            .map_err(|e| feil(&self.sti, e))
    }
}

fn tidspunkt(rad: &Row<'_>, kolonne: usize) -> rusqlite::Result<Timestamp> {
    let sekunder: i64 = rad.get(kolonne)?;
    Timestamp::from_second(sekunder)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(kolonne, Type::Integer, Box::new(e)))
}
