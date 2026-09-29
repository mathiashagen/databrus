//! Rema 1000 offers, including Æ offers (SPEC §4.2). The endpoint is undocumented and
//! must be researched before it is implemented in M3.

use async_trait::async_trait;

use super::{FetchContext, Source, SourceError};
use crate::model::{Chain, RawListing, SourceId};

pub struct Rema;

#[async_trait]
impl Source for Rema {
    fn id(&self) -> SourceId {
        SourceId::Rema
    }

    fn chains(&self) -> &'static [Chain] {
        &[Chain::Rema]
    }

    async fn fetch(&self, _ctx: &FetchContext) -> Result<Vec<RawListing>, SourceError> {
        Err(SourceError::NotImplemented)
    }
}
