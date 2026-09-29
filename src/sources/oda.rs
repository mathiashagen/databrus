//! Oda – a direct adapter for the public JSON endpoints (SPEC §4.2). Implemented in M3.

use async_trait::async_trait;

use super::{FetchContext, Source, SourceError};
use crate::model::{Chain, RawListing, SourceId};

pub const HOST: &str = "oda.com";

pub struct Oda;

#[async_trait]
impl Source for Oda {
    fn id(&self) -> SourceId {
        SourceId::Oda
    }

    fn chains(&self) -> &'static [Chain] {
        &[Chain::Oda]
    }

    async fn fetch(&self, _ctx: &FetchContext) -> Result<Vec<RawListing>, SourceError> {
        Err(SourceError::NotImplemented)
    }
}
