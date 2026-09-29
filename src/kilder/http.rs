//! Felles HTTP-oppsett og høflighet overfor kildene (SPEC §7.4).

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::KildeFeil;

pub const MAKS_SAMTIDIGE_PER_VERT: usize = 2;
pub const MAKS_FORSOK: u32 = 3;
const TILKOBLINGSTIDSAVBRUDD: Duration = Duration::from_secs(10);
const TIDSAVBRUDD: Duration = Duration::from_secs(30);
const MAKS_VENTETID: Duration = Duration::from_secs(30);

/// Ærlig User-Agent med versjon og, når den er satt i Cargo.toml, repo-URL.
pub fn brukeragent() -> String {
    let repo = env!("CARGO_PKG_REPOSITORY");
    let versjon = env!("CARGO_PKG_VERSION");
    if repo.is_empty() {
        format!("databrus/{versjon}")
    } else {
        format!("databrus/{versjon} (+{repo})")
    }
}

/// Hvor lenge vi venter før forsøk nummer `forsok + 1`: `Retry-After` hvis kilden sier
/// noe, ellers eksponentiell backoff fra 0,5 s. Aldri mer enn 30 s.
pub fn ventetid(forsok: u32, retry_after: Option<Duration>) -> Duration {
    retry_after
        .unwrap_or_else(|| Duration::from_millis(500).saturating_mul(1 << forsok.min(10)))
        .min(MAKS_VENTETID)
}

#[derive(Debug, Clone)]
struct Vertsgrense {
    samtidige: Arc<Semaphore>,
    takt: Option<Arc<DefaultDirectRateLimiter>>,
}

impl Default for Vertsgrense {
    fn default() -> Self {
        Self {
            samtidige: Arc::new(Semaphore::new(MAKS_SAMTIDIGE_PER_VERT)),
            takt: None,
        }
    }
}

/// HTTP-klient med grenser per vert: høyst to samtidige forespørsler og, der kilden
/// dokumenterer det, en takt i forespørsler per minutt.
#[derive(Debug)]
pub struct Http {
    pub klient: reqwest::Client,
    verter: Mutex<HashMap<String, Vertsgrense>>,
}

impl Http {
    pub fn ny() -> Result<Self, KildeFeil> {
        let klient = reqwest::Client::builder()
            .user_agent(brukeragent())
            .connect_timeout(TILKOBLINGSTIDSAVBRUDD)
            .timeout(TIDSAVBRUDD)
            .build()?;
        Ok(Self {
            klient,
            verter: Mutex::new(HashMap::new()),
        })
    }

    /// Setter kildens dokumenterte takt for en vert.
    pub fn sett_takt(&self, vert: &str, per_minutt: NonZeroU32) {
        let mut verter = self.verter.lock().unwrap_or_else(PoisonError::into_inner);
        verter.entry(vert.to_owned()).or_default().takt =
            Some(Arc::new(RateLimiter::direct(Quota::per_minute(per_minutt))));
    }

    /// Venter til det er lov å sende en forespørsel til `vert`. Tillatelsen gjelder til
    /// den slippes.
    pub async fn slipp_inn(&self, vert: &str) -> Result<OwnedSemaphorePermit, KildeFeil> {
        let grense = {
            let mut verter = self.verter.lock().unwrap_or_else(PoisonError::into_inner);
            verter.entry(vert.to_owned()).or_default().clone()
        };
        let tillatelse = grense
            .samtidige
            .acquire_owned()
            .await
            .map_err(|_| KildeFeil::Avbrutt)?;
        if let Some(takt) = &grense.takt {
            takt.until_ready().await;
        }
        Ok(tillatelse)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_dobles_og_har_tak() {
        assert_eq!(ventetid(0, None), Duration::from_millis(500));
        assert_eq!(ventetid(1, None), Duration::from_secs(1));
        assert_eq!(ventetid(2, None), Duration::from_secs(2));
        assert_eq!(ventetid(20, None), MAKS_VENTETID);
    }

    #[test]
    fn retry_after_vinner() {
        let fem = Duration::from_secs(5);
        assert_eq!(ventetid(0, Some(fem)), fem);
        assert_eq!(ventetid(0, Some(Duration::from_secs(600))), MAKS_VENTETID);
    }

    #[test]
    fn brukeragent_har_versjon() {
        assert!(brukeragent().starts_with("databrus/"));
    }

    #[tokio::test]
    async fn hoyst_to_samtidige_per_vert() {
        let http = Http::ny().unwrap();
        let a = http.slipp_inn("kassal.app").await.unwrap();
        let _b = http.slipp_inn("kassal.app").await.unwrap();
        let tredje = tokio::time::timeout(Duration::from_millis(50), http.slipp_inn("kassal.app"));
        assert!(tredje.await.is_err(), "tredje forespørsel skulle vente");
        // En annen vert påvirkes ikke.
        let _c = http.slipp_inn("oda.com").await.unwrap();
        drop(a);
        let _d = http.slipp_inn("kassal.app").await.unwrap();
    }
}
