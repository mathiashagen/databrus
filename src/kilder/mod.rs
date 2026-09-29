//! Priskilder (SPEC §4) og orkestrering av hentinger (SPEC §7.3–7.4).
//!
//! Hver kilde implementerer [`Kilde`]. [`oppfrisk`] avgjør hvilke kilder som må hentes
//! (TTL og 15-minuttersgulv), henter dem samtidig, lagrer resultatet og skriver advarsler
//! på stderr for kilder som feiler – uten å stoppe de andre.

pub mod coop;
pub mod http;
pub mod kassalapp;
pub mod oda;
pub mod rema;

use std::sync::Arc;

use async_trait::async_trait;
use jiff::Timestamp;
use owo_colors::OwoColorize;
use serde::Serialize;
use tokio::task::JoinSet;

use crate::feil::AppFeil;
use crate::katalog::Katalog;
use crate::katalog::matching::{GtinIndeks, match_oppforing};
use crate::konfig::{Henting, Konfig};
use crate::lager::{Hentelogg, Lager, Prisobservasjon};
use crate::modell::{self, KildeId, Kjede, RaaOppforing};

pub use http::Http;

#[derive(Debug, thiserror::Error)]
pub enum KildeFeil {
    #[error("ikke implementert ennå")]
    IkkeImplementert,

    #[error("mangler API-nøkkel")]
    ManglerApiNokkel,

    #[error("HTTP {status}")]
    Http { status: u16 },

    #[error("nettverksfeil: {0}")]
    Nettverk(#[from] reqwest::Error),

    #[error("uventet svarformat: {0}")]
    Skjemaendring(String),

    #[error("avbrutt")]
    Avbrutt,
}

/// Det en kilde får med seg når den henter.
#[derive(Debug)]
pub struct HenteKontekst {
    pub http: Http,
    pub api_nokkel: Option<String>,
    /// Kjente EAN-er fra katalogen, normalisert til 14 sifre, for oppslag per produkt.
    pub gtin: Vec<String>,
}

/// En priskilde. En henting gjelder alltid hele energidrikkategorien til kilden, ikke et
/// enkelt søk – søk skjer lokalt, slik at TTL-en gjelder per kilde.
#[async_trait]
pub trait Kilde: Send + Sync {
    fn id(&self) -> KildeId;
    fn kjeder(&self) -> &'static [Kjede];
    async fn hent(&self, ctx: &HenteKontekst) -> Result<Vec<RaaOppforing>, KildeFeil>;
}

pub fn alle() -> Vec<Box<dyn Kilde>> {
    vec![
        Box::new(kassalapp::Kassalapp),
        Box::new(oda::Oda),
        Box::new(rema::Rema),
        Box::new(coop::Coop),
    ]
}

/// Kildene som er slått på i konfigurasjonen, eventuelt begrenset til `kun`.
pub fn aktive(konfig: &Konfig, kun: &[KildeId]) -> Vec<Box<dyn Kilde>> {
    alle()
        .into_iter()
        .filter(|k| konfig.kilder.er_aktiv(k.id()) && (kun.is_empty() || kun.contains(&k.id())))
        .collect()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Hentemodus {
    /// Hent selv om dataene er ferske (`--oppdater`, `oppdater`). Gulvet gjelder likevel.
    pub tving: bool,
    /// Aldri nettverk (`--frakoblet`).
    pub frakoblet: bool,
    /// Ingen informasjonsmeldinger, bare advarsler og feil.
    pub stille: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Beslutning {
    Hent,
    Fersk,
    Frakoblet,
    ForTidlig { vent_minutter: u32 },
}

/// Avgjør om en kilde skal hentes nå (SPEC §7.3–7.4).
pub fn vurder_henting(
    modus: Hentemodus,
    siste_forsok: Option<Timestamp>,
    siste_vellykkede: Option<Timestamp>,
    na: Timestamp,
    henting: &Henting,
) -> Beslutning {
    if modus.frakoblet {
        return Beslutning::Frakoblet;
    }
    let gulv = i64::from(henting.min_intervall_minutter);
    if let Some(forsok) = siste_forsok {
        let gatt = minutter_mellom(forsok, na);
        if gatt < gulv {
            return Beslutning::ForTidlig {
                vent_minutter: u32::try_from(gulv - gatt).unwrap_or(0),
            };
        }
    }
    if !modus.tving
        && let Some(ok) = siste_vellykkede
        && minutter_mellom(ok, na) < i64::from(henting.ttl_timer) * 60
    {
        return Beslutning::Fersk;
    }
    Beslutning::Hent
}

fn minutter_mellom(fra: Timestamp, til: Timestamp) -> i64 {
    til.duration_since(fra).as_secs() / 60
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kildestatus {
    Ok,
    Feilet,
    AldriHentet,
}

/// Status for én kilde, slik den vises i `kilder` i JSON (SPEC §9.1).
#[derive(Debug, Clone, Serialize)]
pub struct KildeInfo {
    pub id: KildeId,
    pub status: Kildestatus,
    /// Siste vellykkede henting – alderen på dataene.
    pub hentet: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feil: Option<String>,
    /// Oppføringer hentet i denne kjøringen.
    #[serde(skip)]
    pub nye_oppforinger: Option<usize>,
}

impl KildeInfo {
    fn fra_logg(
        id: KildeId,
        siste: Option<&Hentelogg>,
        siste_vellykkede: Option<Timestamp>,
    ) -> Self {
        let (status, feil) = match siste {
            None => (Kildestatus::AldriHentet, None),
            Some(logg) if logg.ok => (Kildestatus::Ok, None),
            Some(logg) => (Kildestatus::Feilet, logg.feilmelding.clone()),
        };
        Self {
            id,
            status,
            hentet: siste_vellykkede,
            feil,
            nye_oppforinger: None,
        }
    }
}

/// Henter fra kildene som trenger det, og returnerer status for alle aktive kilder.
pub async fn oppfrisk(
    lager: &mut Lager,
    konfig: &Konfig,
    katalog: &Katalog,
    modus: Hentemodus,
    kun: &[KildeId],
) -> Result<Vec<KildeInfo>, AppFeil> {
    let na = modell::na();
    let mut info = Vec::new();
    let mut skal_hentes = Vec::new();

    for kilde in aktive(konfig, kun) {
        let id = kilde.id();
        let siste = lager.siste_henting(id)?;
        let siste_vellykkede = lager.siste_vellykkede_henting(id)?;
        let beslutning = vurder_henting(
            modus,
            siste.as_ref().map(|h| h.startet),
            siste_vellykkede,
            na,
            &konfig.henting,
        );
        if beslutning == Beslutning::Hent {
            skal_hentes.push(kilde);
            continue;
        }
        if let Beslutning::ForTidlig { vent_minutter } = beslutning
            && modus.tving
            && !modus.stille
        {
            anstream::eprintln!(
                "{} {}: hentet nylig – neste henting tidligst om {vent_minutter} min",
                "info:".cyan().bold(),
                id.visningsnavn()
            );
        }
        let info_fra_logg = KildeInfo::fra_logg(id, siste.as_ref(), siste_vellykkede);
        if let Some(feil) = &info_fra_logg.feil {
            advarsel(id, &format!("feilet ved siste henting ({feil})"));
        }
        info.push(info_fra_logg);
    }

    if !skal_hentes.is_empty() {
        let http = Http::ny().map_err(|feil| AppFeil::Nettverk(feil.to_string()))?;
        let ctx = Arc::new(HenteKontekst {
            http,
            api_nokkel: konfig.api_nokkel(),
            gtin: katalog.alle_gtin(),
        });

        let mut oppgaver = JoinSet::new();
        for kilde in skal_hentes {
            let ctx = Arc::clone(&ctx);
            oppgaver.spawn(async move {
                let startet = modell::na();
                let svar = kilde.hent(&ctx).await;
                (kilde.id(), startet, svar)
            });
        }
        let mut ferdige = Vec::new();
        while let Some(resultat) = oppgaver.join_next().await {
            match resultat {
                Ok(ferdig) => ferdige.push(ferdig),
                Err(feil) => tracing::error!("henteoppgave stoppet uventet: {feil}"),
            }
        }
        ferdige.sort_by_key(|(id, ..)| *id);

        let indeks = GtinIndeks::bygg(katalog);
        for (id, startet, svar) in ferdige {
            let fullfort = modell::na();
            match svar {
                Ok(oppforinger) => {
                    let antall = lagre(lager, katalog, &indeks, &oppforinger, fullfort)?;
                    lager.logg_henting(id, startet, fullfort, Ok(antall))?;
                    info.push(KildeInfo {
                        id,
                        status: Kildestatus::Ok,
                        hentet: Some(fullfort),
                        feil: None,
                        nye_oppforinger: Some(antall),
                    });
                }
                Err(feil) => {
                    let melding = feil.to_string();
                    lager.logg_henting(id, startet, fullfort, Err(&melding))?;
                    advarsel(id, &melding);
                    if matches!(feil, KildeFeil::ManglerApiNokkel) {
                        anstream::eprintln!(
                            "  Hent en gratis nøkkel på https://kassal.app/api og sett {},\n  \
                             eller kjør: databrus konfig sett kilder.kassalapp.api_nokkel <NØKKEL>",
                            crate::konfig::ENV_API_NOKKEL
                        );
                    }
                    info.push(KildeInfo {
                        id,
                        status: Kildestatus::Feilet,
                        hentet: lager.siste_vellykkede_henting(id)?,
                        feil: Some(melding),
                        nye_oppforinger: None,
                    });
                }
            }
        }
    }

    info.sort_by_key(|i| i.id);
    Ok(info)
}

/// Lagrer en kildes oppføringer med endringsbasert historikk.
fn lagre(
    lager: &mut Lager,
    katalog: &Katalog,
    indeks: &GtinIndeks,
    oppforinger: &[RaaOppforing],
    na: Timestamp,
) -> Result<usize, AppFeil> {
    lager.synk_katalog(katalog)?;
    for oppforing in oppforinger {
        let treff = match_oppforing(indeks, oppforing);
        let id = lager.lagre_oppforing(oppforing, treff.as_ref(), na)?;
        // Fornuftssjekken (SPEC §5.5) kobles på i M1, når prisene beregnes ved lagring.
        lager.registrer_pris(id, &Prisobservasjon::fra(oppforing), na)?;
    }
    Ok(oppforinger.len())
}

fn advarsel(id: KildeId, melding: &str) {
    anstream::eprintln!(
        "{} {}: {melding}",
        "advarsel:".yellow().bold(),
        id.visningsnavn()
    );
}

#[cfg(test)]
mod tests {
    use jiff::ToSpan;

    use super::*;

    fn tid(minutter_siden: i64) -> Option<Timestamp> {
        Some(na() - minutter_siden.minutes())
    }

    fn na() -> Timestamp {
        "2026-09-29T12:00:00Z".parse().unwrap()
    }

    fn vurder(modus: Hentemodus, forsok: Option<Timestamp>, ok: Option<Timestamp>) -> Beslutning {
        vurder_henting(modus, forsok, ok, na(), &Henting::default())
    }

    #[test]
    fn aldri_hentet_hentes() {
        assert_eq!(vurder(Hentemodus::default(), None, None), Beslutning::Hent);
    }

    #[test]
    fn frakoblet_henter_aldri() {
        let modus = Hentemodus {
            frakoblet: true,
            tving: true,
            ..Hentemodus::default()
        };
        assert_eq!(vurder(modus, None, None), Beslutning::Frakoblet);
    }

    #[test]
    fn ferske_data_hentes_ikke() {
        assert_eq!(
            vurder(Hentemodus::default(), tid(60), tid(60)),
            Beslutning::Fersk
        );
        assert_eq!(
            vurder(Hentemodus::default(), tid(7 * 60), tid(7 * 60)),
            Beslutning::Hent
        );
    }

    #[test]
    fn tving_hopper_over_ttl_men_ikke_gulvet() {
        let tving = Hentemodus {
            tving: true,
            ..Hentemodus::default()
        };
        assert_eq!(vurder(tving, tid(60), tid(60)), Beslutning::Hent);
        assert_eq!(
            vurder(tving, tid(5), tid(5)),
            Beslutning::ForTidlig { vent_minutter: 10 }
        );
    }

    #[test]
    fn feilet_forsok_teller_mot_gulvet() {
        assert_eq!(
            vurder(Hentemodus::default(), tid(3), None),
            Beslutning::ForTidlig { vent_minutter: 12 }
        );
    }
}
