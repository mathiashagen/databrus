//! Rema 1000-tilbud, inkludert Æ-tilbud (SPEC §4.2). Endepunktet er udokumentert og må
//! undersøkes før implementering i M3.

use async_trait::async_trait;

use super::{HenteKontekst, Kilde, KildeFeil};
use crate::modell::{KildeId, Kjede, RaaOppforing};

pub struct Rema;

#[async_trait]
impl Kilde for Rema {
    fn id(&self) -> KildeId {
        KildeId::Rema
    }

    fn kjeder(&self) -> &'static [Kjede] {
        &[Kjede::Rema]
    }

    async fn hent(&self, _ctx: &HenteKontekst) -> Result<Vec<RaaOppforing>, KildeFeil> {
        Err(KildeFeil::IkkeImplementert)
    }
}
