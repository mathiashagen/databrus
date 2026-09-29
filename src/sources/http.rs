//! Shared HTTP setup and politeness toward the sources (SPEC §7.4).

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use reqwest::header::{ACCEPT, RETRY_AFTER};
use reqwest::{Response, StatusCode, Url};
use serde::de::DeserializeOwned;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::SourceError;

pub const MAX_CONCURRENT_PER_HOST: usize = 2;
/// Retries after the first attempt, on 429, 5xx and network failures (SPEC §7.4).
pub const MAX_RETRIES: u32 = 3;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const TIMEOUT: Duration = Duration::from_secs(30);
const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// An honest User-Agent: it says it is a bot, names the program and gives the repo URL as
/// the contact. Oda requires all three for automated clients (SPEC §4.6).
pub fn user_agent() -> String {
    let repo = env!("CARGO_PKG_REPOSITORY");
    let version = env!("CARGO_PKG_VERSION");
    if repo.is_empty() {
        format!("databrus-bot/{version}")
    } else {
        format!("databrus-bot/{version} (+{repo})")
    }
}

/// How long to wait before attempt `attempt + 1`: `Retry-After` if the source says so,
/// otherwise exponential backoff from 0.5 s. Never more than 30 s.
pub fn backoff(attempt: u32, retry_after: Option<Duration>) -> Duration {
    retry_after
        .unwrap_or_else(|| Duration::from_millis(500).saturating_mul(1 << attempt.min(10)))
        .min(MAX_BACKOFF)
}

#[derive(Debug, Clone)]
struct HostLimit {
    concurrent: Arc<Semaphore>,
    rate: Option<Arc<DefaultDirectRateLimiter>>,
}

impl Default for HostLimit {
    fn default() -> Self {
        Self {
            concurrent: Arc::new(Semaphore::new(MAX_CONCURRENT_PER_HOST)),
            rate: None,
        }
    }
}

/// HTTP client with per-host limits: at most two concurrent requests and, where the
/// source documents it, a rate in requests per minute.
#[derive(Debug)]
pub struct Http {
    pub client: reqwest::Client,
    hosts: Mutex<HashMap<String, HostLimit>>,
}

impl Http {
    pub fn new() -> Result<Self, SourceError> {
        let client = reqwest::Client::builder()
            .user_agent(user_agent())
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(TIMEOUT)
            .build()?;
        Ok(Self {
            client,
            hosts: Mutex::new(HashMap::new()),
        })
    }

    /// Sets the source's documented rate for a host.
    pub fn set_rate(&self, host: &str, per_minute: NonZeroU32) {
        let mut hosts = self.hosts.lock().unwrap_or_else(PoisonError::into_inner);
        hosts.entry(host.to_owned()).or_default().rate =
            Some(Arc::new(RateLimiter::direct(Quota::per_minute(per_minute))));
    }

    /// GET as JSON, with the per-host limits and retries on 429, 5xx and network
    /// failures. 401/403 give [`SourceError::RejectedKey`]; other 4xx fail immediately.
    pub async fn get_json<T: DeserializeOwned>(
        &self,
        url: &str,
        bearer: Option<&str>,
    ) -> Result<T, SourceError> {
        let host = Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned))
            .ok_or_else(|| SourceError::InvalidUrl(url.to_owned()))?;

        let mut attempt = 0;
        loop {
            let permit = self.acquire(&host).await?;
            let mut request = self.client.get(url).header(ACCEPT, "application/json");
            if let Some(key) = bearer {
                request = request.bearer_auth(key);
            }
            let (error, retry_after) = match request.send().await {
                Ok(response) if response.status().is_success() => {
                    let text = response.text().await?;
                    return serde_json::from_str(&text)
                        .map_err(|error| SourceError::SchemaChange(error.to_string()));
                }
                Ok(response) => {
                    let status = response.status();
                    if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
                        return Err(SourceError::RejectedKey {
                            status: status.as_u16(),
                        });
                    }
                    let error = SourceError::Http {
                        status: status.as_u16(),
                    };
                    if status != StatusCode::TOO_MANY_REQUESTS && !status.is_server_error() {
                        return Err(error);
                    }
                    (error, retry_after(&response))
                }
                Err(error) if error.is_timeout() || error.is_connect() => {
                    (SourceError::Network(error), None)
                }
                Err(error) => return Err(error.into()),
            };
            drop(permit);

            if attempt >= MAX_RETRIES {
                return Err(error);
            }
            let wait = backoff(attempt, retry_after);
            tracing::debug!("{host}: {error} – nytt forsøk om {wait:?}");
            tokio::time::sleep(wait).await;
            attempt += 1;
        }
    }

    /// Waits until a request to `host` is allowed. The permit holds until it is dropped.
    pub async fn acquire(&self, host: &str) -> Result<OwnedSemaphorePermit, SourceError> {
        let limit = {
            let mut hosts = self.hosts.lock().unwrap_or_else(PoisonError::into_inner);
            hosts.entry(host.to_owned()).or_default().clone()
        };
        let permit = limit
            .concurrent
            .acquire_owned()
            .await
            .map_err(|_| SourceError::Cancelled)?;
        if let Some(rate) = &limit.rate {
            rate.until_ready().await;
        }
        Ok(permit)
    }
}

/// `Retry-After` in seconds. The HTTP-date form is not used by our sources and is ignored.
fn retry_after(response: &Response) -> Option<Duration> {
    response
        .headers()
        .get(RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()
        .map(Duration::from_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_and_is_capped() {
        assert_eq!(backoff(0, None), Duration::from_millis(500));
        assert_eq!(backoff(1, None), Duration::from_secs(1));
        assert_eq!(backoff(2, None), Duration::from_secs(2));
        assert_eq!(backoff(20, None), MAX_BACKOFF);
    }

    #[test]
    fn retry_after_wins() {
        let five = Duration::from_secs(5);
        assert_eq!(backoff(0, Some(five)), five);
        assert_eq!(backoff(0, Some(Duration::from_secs(600))), MAX_BACKOFF);
    }

    #[test]
    fn user_agent_follows_odas_policy() {
        let ua = user_agent();
        assert!(ua.starts_with("databrus-bot/"), "{ua}");
        assert!(ua.contains("github.com/mathiashagen/databrus"), "{ua}");
    }

    #[tokio::test]
    async fn at_most_two_concurrent_per_host() {
        let http = Http::new().unwrap();
        let a = http.acquire("kassal.app").await.unwrap();
        let _b = http.acquire("kassal.app").await.unwrap();
        let third = tokio::time::timeout(Duration::from_millis(50), http.acquire("kassal.app"));
        assert!(third.await.is_err(), "the third request should wait");
        // Another host is not affected.
        let _c = http.acquire("oda.com").await.unwrap();
        drop(a);
        let _d = http.acquire("kassal.app").await.unwrap();
    }
}
