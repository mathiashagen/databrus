//! Kassalapp-API-et – primærkilden for grunnpriser (SPEC §4.2). Implementeres i M1.

use async_trait::async_trait;

use super::{HenteKontekst, Kilde, KildeFeil};
use crate::modell::{KildeId, Kjede, RaaOppforing};

pub const BASIS_URL: &str = "https://kassal.app/api/v1";
pub const VERT: &str = "kassal.app";
/// Grensen for gratisnøkler. Må bekreftes mot dokumentasjonen ved implementering.
pub const FORESPORSLER_PER_MINUTT: u32 = 60;

pub struct Kassalapp;

#[async_trait]
impl Kilde for Kassalapp {
    fn id(&self) -> KildeId {
        KildeId::Kassalapp
    }

    fn kjeder(&self) -> &'static [Kjede] {
        &Kjede::ALLE
    }

    async fn hent(&self, ctx: &HenteKontekst) -> Result<Vec<RaaOppforing>, KildeFeil> {
        if ctx.api_nokkel.is_none() {
            return Err(KildeFeil::ManglerApiNokkel);
        }
        Err(KildeFeil::IkkeImplementert)
    }
}
