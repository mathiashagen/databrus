//! Lesing. Spørringene for historikk og eksport kommer i M2.

use std::collections::HashMap;

use jiff::Timestamp;
use rusqlite::types::Type;
use rusqlite::{OptionalExtension, Row};
use serde::Serialize;

use super::{Lager, feil};
use crate::feil::AppFeil;
use crate::modell::{KildeId, Kjede, Medlemspris, Medlemsprogram, Ore, ProduktId, Tilbudsinfo};

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

/// Gjeldende pris for en oppføring som matchet katalogen: det nyeste prisintervallet.
#[derive(Debug, Clone, PartialEq)]
pub struct LagretPris {
    pub oppforing_id: i64,
    pub kilde: KildeId,
    pub kjede: Kjede,
    pub produkt: ProduktId,
    /// Antall beholdere i den salgbare enheten.
    pub antall: u32,
    pub verifisert: bool,
    pub hyllepris: Ore,
    pub medlemspris: Option<Medlemspris>,
    pub tilbud: Option<Tilbudsinfo>,
    pub tilgjengelig: Option<bool>,
    pub mistenkelig: bool,
    /// Når kilden sist så prisen.
    pub sist_sett: Timestamp,
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

    /// Nyeste pris for hver oppføring som matchet et katalogprodukt. Rader med ukjent
    /// kilde, kjede eller medlemsprogram (f.eks. fra en nyere versjon) hoppes over, og et
    /// tilbud som ikke kan tolkes, ignoreres med en logglinje.
    pub fn siste_priser(&self) -> Result<Vec<LagretPris>, AppFeil> {
        let mut sporring = self
            .conn
            .prepare(
                "SELECT o.id, o.kilde, o.kjede, o.produkt_id, o.antall, o.verifisert,
                        p.hyllepris_ore, p.medlemspris_ore, p.medlemsprogram, p.tilbud_json,
                        p.tilgjengelig, p.mistenkelig, p.sist_sett
                 FROM oppforing o
                 JOIN prisintervall p ON p.id = (
                     SELECT id FROM prisintervall WHERE oppforing_id = o.id
                     ORDER BY gyldig_fra DESC, id DESC LIMIT 1)
                 WHERE o.produkt_id IS NOT NULL",
            )
            .map_err(|e| feil(&self.sti, e))?;
        let rader = sporring
            .query_map([], |rad| {
                Ok((
                    rad.get::<_, i64>(0)?,
                    rad.get::<_, String>(1)?,
                    rad.get::<_, String>(2)?,
                    rad.get::<_, String>(3)?,
                    rad.get::<_, u32>(4)?,
                    rad.get::<_, bool>(5)?,
                    rad.get::<_, i64>(6)?,
                    rad.get::<_, Option<i64>>(7)?,
                    rad.get::<_, Option<String>>(8)?,
                    rad.get::<_, Option<String>>(9)?,
                    rad.get::<_, Option<bool>>(10)?,
                    rad.get::<_, bool>(11)?,
                    tidspunkt(rad, 12)?,
                ))
            })
            .map_err(|e| feil(&self.sti, e))?;

        let mut priser = Vec::new();
        for rad in rader {
            let (
                id,
                kilde,
                kjede,
                produkt,
                antall,
                verifisert,
                hyllepris,
                medlemspris,
                program,
                tilbud,
                tilgjengelig,
                mistenkelig,
                sist_sett,
            ) = rad.map_err(|e| feil(&self.sti, e))?;
            let (Some(kilde), Some(kjede)) = (KildeId::fra_slug(&kilde), Kjede::fra_slug(&kjede))
            else {
                continue;
            };
            let medlemspris = match (
                medlemspris,
                program.as_deref().map(Medlemsprogram::fra_slug),
            ) {
                (Some(pris), Some(Some(program))) => Some(Medlemspris {
                    pris: Ore(pris),
                    program,
                }),
                _ => None,
            };
            let tilbud = tilbud.and_then(|json| {
                serde_json::from_str(&json)
                    .inspect_err(|e| tracing::debug!("oppføring {id}: ugyldig tilbud ({e})"))
                    .ok()
            });
            priser.push(LagretPris {
                oppforing_id: id,
                kilde,
                kjede,
                produkt: ProduktId(produkt),
                antall: antall.max(1),
                verifisert,
                hyllepris: Ore(hyllepris),
                medlemspris,
                tilbud,
                tilgjengelig,
                mistenkelig,
                sist_sett,
            });
        }
        Ok(priser)
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
