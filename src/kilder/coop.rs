//! Coop-tilbud for Extra, Obs, Mega og Prix, med medlemspriser (SPEC §4.2). Endepunktene
//! er udokumenterte og må undersøkes før implementering i M3.

use async_trait::async_trait;

use super::{HenteKontekst, Kilde, KildeFeil};
use crate::modell::{KildeId, Kjede, RaaOppforing};

pub struct Coop;

#[async_trait]
impl Kilde for Coop {
    fn id(&self) -> KildeId {
        KildeId::Coop
    }

    fn kjeder(&self) -> &'static [Kjede] {
        &Kjede::COOP
    }

    async fn hent(&self, _ctx: &HenteKontekst) -> Result<Vec<RaaOppforing>, KildeFeil> {
        Err(KildeFeil::IkkeImplementert)
    }
}
