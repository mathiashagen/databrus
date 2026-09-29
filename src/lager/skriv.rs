//! Skriving: produkter, oppføringer, endringsbasert prishistorikk og hentelogg.

use jiff::Timestamp;
use rusqlite::{Connection, OptionalExtension, params};

use super::{Lager, feil};
use crate::feil::AppFeil;
use crate::katalog::Katalog;
use crate::katalog::matching::Treff;
use crate::modell::{KildeId, Medlemspris, Ore, Produkt, RaaOppforing, Tilbudsinfo};

/// Det som lagres per prisobservasjon.
#[derive(Debug, Clone, PartialEq)]
pub struct Prisobservasjon {
    pub hyllepris: Ore,
    pub medlemspris: Option<Medlemspris>,
    pub tilbud: Option<Tilbudsinfo>,
    pub tilgjengelig: Option<bool>,
    pub mistenkelig: bool,
}

impl Prisobservasjon {
    pub fn fra(oppforing: &RaaOppforing) -> Self {
        Self {
            hyllepris: oppforing.hyllepris,
            medlemspris: oppforing.medlemspris,
            tilbud: oppforing.tilbud.clone(),
            tilgjengelig: oppforing.tilgjengelig,
            mistenkelig: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endring {
    /// Samme pris som sist – bare `sist_sett` ble oppdatert.
    Uendret,
    /// Ny pris – et nytt intervall ble lagt til.
    NyttIntervall(i64),
}

/// Feltene som avgjør om en observasjon er en endring.
#[derive(Debug, PartialEq)]
struct Prisnokkel {
    hyllepris: i64,
    medlemspris: Option<i64>,
    medlemsprogram: Option<String>,
    tilbud_json: Option<String>,
    tilgjengelig: Option<bool>,
}

impl Lager {
    pub fn lagre_produkt(&self, produkt: &Produkt, ad_hoc: bool) -> Result<(), AppFeil> {
        skriv_produkt(&self.conn, produkt, ad_hoc).map_err(|e| feil(&self.sti, e))
    }

    /// Sørger for at alle katalogprodukter finnes i databasen (oppføringer peker på dem).
    pub fn synk_katalog(&mut self, katalog: &Katalog) -> Result<(), AppFeil> {
        let tx = self.conn.transaction().map_err(|e| feil(&self.sti, e))?;
        for produkt in &katalog.produkter {
            skriv_produkt(&tx, produkt, false).map_err(|e| feil(&self.sti, e))?;
        }
        tx.commit().map_err(|e| feil(&self.sti, e))
    }

    /// Setter inn eller oppdaterer en oppføring og returnerer id-en.
    pub fn lagre_oppforing(
        &self,
        oppforing: &RaaOppforing,
        treff: Option<&Treff>,
        na: Timestamp,
    ) -> Result<i64, AppFeil> {
        self.conn
            .query_row(
                "INSERT INTO oppforing
                     (kilde, kjede, kilde_produkt_id, gtin, produkt_id, antall, raanavn,
                      verifisert, forst_sett, sist_sett)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
                 ON CONFLICT (kilde, kjede, kilde_produkt_id) DO UPDATE SET
                     gtin = excluded.gtin, produkt_id = excluded.produkt_id,
                     antall = excluded.antall, raanavn = excluded.raanavn,
                     verifisert = excluded.verifisert, sist_sett = excluded.sist_sett
                 RETURNING id",
                params![
                    oppforing.kilde.slug(),
                    oppforing.kjede.slug(),
                    oppforing.kilde_produkt_id,
                    oppforing.gtin,
                    treff.map(|t| &t.produkt.0),
                    treff.map_or(oppforing.antall, |t| t.antall).max(1),
                    oppforing.raanavn,
                    treff.is_some_and(|t| t.verifisert),
                    na.as_second(),
                ],
                |rad| rad.get(0),
            )
            .map_err(|e| feil(&self.sti, e))
    }

    /// Registrerer en prisobservasjon. Er den lik det nyeste intervallet, oppdateres bare
    /// `sist_sett`; ellers starter et nytt intervall (SPEC §7.2).
    pub fn registrer_pris(
        &mut self,
        oppforing_id: i64,
        observasjon: &Prisobservasjon,
        na: Timestamp,
    ) -> Result<Endring, AppFeil> {
        let tilbud_json = observasjon
            .tilbud
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let ny = Prisnokkel {
            hyllepris: observasjon.hyllepris.0,
            medlemspris: observasjon.medlemspris.map(|m| m.pris.0),
            medlemsprogram: observasjon.medlemspris.map(|m| m.program.slug().to_owned()),
            tilbud_json,
            tilgjengelig: observasjon.tilgjengelig,
        };

        let tx = self.conn.transaction().map_err(|e| feil(&self.sti, e))?;
        let siste = tx
            .query_row(
                "SELECT id, hyllepris_ore, medlemspris_ore, medlemsprogram, tilbud_json, tilgjengelig
                 FROM prisintervall WHERE oppforing_id = ?1
                 ORDER BY gyldig_fra DESC, id DESC LIMIT 1",
                [oppforing_id],
                |rad| {
                    Ok((
                        rad.get::<_, i64>(0)?,
                        Prisnokkel {
                            hyllepris: rad.get(1)?,
                            medlemspris: rad.get(2)?,
                            medlemsprogram: rad.get(3)?,
                            tilbud_json: rad.get(4)?,
                            tilgjengelig: rad.get(5)?,
                        },
                    ))
                },
            )
            .optional()
            .map_err(|e| feil(&self.sti, e))?;

        let endring = match siste {
            Some((id, forrige)) if forrige == ny => {
                tx.execute(
                    "UPDATE prisintervall SET sist_sett = ?1 WHERE id = ?2",
                    params![na.as_second(), id],
                )
                .map_err(|e| feil(&self.sti, e))?;
                Endring::Uendret
            }
            _ => {
                let id = tx
                    .query_row(
                        "INSERT INTO prisintervall
                             (oppforing_id, hyllepris_ore, medlemspris_ore, medlemsprogram,
                              tilbud_json, tilgjengelig, mistenkelig, gyldig_fra, sist_sett)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
                         RETURNING id",
                        params![
                            oppforing_id,
                            ny.hyllepris,
                            ny.medlemspris,
                            ny.medlemsprogram,
                            ny.tilbud_json,
                            ny.tilgjengelig,
                            observasjon.mistenkelig,
                            na.as_second(),
                        ],
                        |rad| rad.get(0),
                    )
                    .map_err(|e| feil(&self.sti, e))?;
                Endring::NyttIntervall(id)
            }
        };
        tx.commit().map_err(|e| feil(&self.sti, e))?;
        Ok(endring)
    }

    /// Logger et henteforsøk. `resultat` er antall oppføringer, eller feilmeldingen.
    pub fn logg_henting(
        &self,
        kilde: KildeId,
        startet: Timestamp,
        fullfort: Timestamp,
        resultat: Result<usize, &str>,
    ) -> Result<(), AppFeil> {
        let (status, feilmelding, antall) = match resultat {
            Ok(antall) => ("ok", None, i64::try_from(antall).ok()),
            Err(melding) => ("feilet", Some(melding), None),
        };
        self.conn
            .execute(
                "INSERT INTO henting (kilde, startet, fullfort, status, feilmelding, antall_oppforinger)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    kilde.slug(),
                    startet.as_second(),
                    fullfort.as_second(),
                    status,
                    feilmelding,
                    antall,
                ],
            )
            .map_err(|e| feil(&self.sti, e))?;
        Ok(())
    }
}

fn skriv_produkt(conn: &Connection, produkt: &Produkt, ad_hoc: bool) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO produkt
             (id, navn, merke, linje, smak, sukkerfri, volum_ml, beholder, egenmerke, ad_hoc)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT (id) DO UPDATE SET
             navn = excluded.navn, merke = excluded.merke, linje = excluded.linje,
             smak = excluded.smak, sukkerfri = excluded.sukkerfri,
             volum_ml = excluded.volum_ml, beholder = excluded.beholder,
             egenmerke = excluded.egenmerke, ad_hoc = excluded.ad_hoc",
        params![
            produkt.id.0,
            produkt.navn,
            produkt.merke,
            produkt.linje,
            produkt.smak,
            produkt.sukkerfri,
            produkt.volum_ml.get(),
            produkt.beholder.slug(),
            produkt.egenmerke,
            ad_hoc,
        ],
    )?;
    Ok(())
}
