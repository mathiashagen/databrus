//! Coop offers for Extra, Obs, Mega and Prix, with member prices (SPEC §4.2). The
//! endpoints are undocumented and must be researched before they are implemented in M3.

use async_trait::async_trait;

use super::{FetchContext, Source, SourceError};
use crate::model::{Chain, RawListing, SourceId};

pub struct Coop;

#[async_trait]
impl Source for Coop {
    fn id(&self) -> SourceId {
        SourceId::Coop
    }

    fn chains(&self) -> &'static [Chain] {
        &Chain::COOP
    }

    async fn fetch(&self, _ctx: &FetchContext) -> Result<Vec<RawListing>, SourceError> {
        Err(SourceError::NotImplemented)
    }
}
