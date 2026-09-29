//! Oda – direkte adapter mot de offentlige JSON-endepunktene (SPEC §4.2). Implementeres i M3.

use async_trait::async_trait;

use super::{HenteKontekst, Kilde, KildeFeil};
use crate::modell::{KildeId, Kjede, RaaOppforing};

pub const VERT: &str = "oda.com";

pub struct Oda;

#[async_trait]
impl Kilde for Oda {
    fn id(&self) -> KildeId {
        KildeId::Oda
    }

    fn kjeder(&self) -> &'static [Kjede] {
        &[Kjede::Oda]
    }

    async fn hent(&self, _ctx: &HenteKontekst) -> Result<Vec<RaaOppforing>, KildeFeil> {
        Err(KildeFeil::IkkeImplementert)
    }
}
