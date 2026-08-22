//! The fetch pool: one place that turns a URL into a response, with every
//! politeness rule already applied.
//!
//! Assembles the three preceding pieces. robots.txt decides whether the URL may
//! be fetched at all, the limiter decides when, and the retry policy decides
//! whether a failure is worth another go. Nothing else in Pounce sends an HTTP
//! request, because anything that did would bypass all three.
//!
//! A failing status is not a failure. A 404 is a broken link and a 503 that
//! outlived its retries is a struggling server: both are findings the audit
//! wants recorded, so both come back as `Ok`. Only a request that never
//! produced a response at all is an `Err`.

use crate::limit::Limiter;
use crate::retry::RetryPolicy;
use crate::robots::{Access, RobotsCache};
use pounce_core::CrawlUrl;
use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE, HeaderMap};
use reqwest::{Client, Method, StatusCode};
use std::time::{Duration, Instant};
use tokio::sync::OwnedSemaphorePermit;

#[derive(Debug, Clone)]
pub struct FetchConfig {
    /// Ceiling on a single attempt, so one hung connection cannot park a
    /// worker for the rest of the crawl.
    pub timeout: Duration,
    pub max_concurrent_per_host: usize,
    /// Spacing used for a host whose robots.txt states no `Crawl-delay`.
    pub default_delay: Duration,
    /// Bodies are read to this and then truncated. A crawler must survive a
    /// hostile or accidental multi-gigabyte response without buffering it.
    pub max_body_bytes: usize,
    /// Hops `follow` will cross before giving up. Ten matches what browsers
    /// and `reqwest` allow; a chain longer than that is a misconfiguration
    /// worth reporting rather than a route worth completing.
    pub max_redirects: usize,
}

impl Default for FetchConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            max_concurrent_per_host: 4,
            default_delay: Duration::ZERO,
            max_body_bytes: 8 * 1024 * 1024,
            max_redirects: 10,
        }
    }
}

/// One response, read to completion.
///
/// The body is read inside the fetcher rather than handed back as a stream, so
/// that the host's concurrency permit covers the whole request including the
/// body. A permit released before the body is drained is not a concurrency
/// limit.
#[derive(Debug)]
pub struct Fetched {
    pub url: CrawlUrl,
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
    /// True when the response was longer than `max_body_bytes` and `body` holds
    /// only the leading part of it.
    pub truncated: bool,
    /// Wall time for the attempt that produced this response, excluding any
    /// politeness wait before it.
    pub elapsed: Duration,
    /// Time until the status line and headers arrived, excluding the body.
    ///
    /// Kept separate from `elapsed` because they diagnose different things: a
    /// slow `time_to_headers` is a slow server, while a large gap between the
    /// two is a large or slowly streamed page. Collapsing them into one number
    /// makes a 2 MB page indistinguishable from an overloaded backend.
    pub time_to_headers: Duration,
    /// The `Content-Length` the server declared, if it declared one. Compared
    /// against `body.len()` this catches truncated transfers; a chunked
    /// response simply has none.
    pub declared_length: Option<u64>,
}

impl Fetched {
    /// Bytes actually read, after any truncation at `max_body_bytes`.
    pub fn size(&self) -> usize {
        self.body.len()
    }

    /// The `Content-Type` header exactly as sent, parameters included.
    pub fn content_type(&self) -> Option<&str> {
        self.headers.get(CONTENT_TYPE)?.to_str().ok()
    }

    /// The media type alone, lowercased: `text/html` from
    /// `Text/HTML; charset=UTF-8`.
    ///
    /// Parsed here rather than with a MIME crate because the crawler only ever
    /// asks two questions of this header — what type, what charset — and a
    /// dependency that models the whole grammar earns nothing for them.
    pub fn mime(&self) -> Option<String> {
        mime_of(&self.headers)
    }

    /// The declared `charset` parameter, lowercased.
    pub fn charset(&self) -> Option<String> {
        self.content_type()?.split(';').skip(1).find_map(|param| {
            let (key, value) = param.split_once('=')?;
            key.trim()
                .eq_ignore_ascii_case("charset")
                .then(|| value.trim().trim_matches('"').to_ascii_lowercase())
        })
    }

    /// Whether this response should go to the HTML parser.
    ///
    /// `application/xhtml+xml` counts; a missing `Content-Type` does not.
    /// Sniffing bodies with no declared type is deliberately left out — it
    /// guesses, and a crawl report that guesses is worse than one that says
    /// the server declared nothing.
    pub fn is_html(&self) -> bool {
        matches!(
            self.mime().as_deref(),
            Some("text/html" | "application/xhtml+xml")
        )
    }
}

/// The media type alone, lowercased: `text/html` from `Text/HTML; charset=UTF-8`.
///
/// One function so a `GET` and a `HEAD` can never disagree about what a server
/// said — an image reported as `image/jpeg` by one and `Image/JPEG` by the
/// other would be two rows for one resource.
fn mime_of(headers: &HeaderMap) -> Option<String> {
    let essence = headers
        .get(CONTENT_TYPE)?
        .to_str()
        .ok()?
        .split(';')
        .next()?
        .trim()
        .to_ascii_lowercase();
    (!essence.is_empty()).then_some(essence)
}

/// What a `HEAD` establishes about a URL the crawl will never parse.
///
/// Deliberately not a `Fetched` with an empty body. A `Fetched` carries
/// timings, a charset, and a truncation flag that a `HEAD` cannot honestly
/// fill in, and a caller handed one would have no way to tell a body that was
/// empty from one that was never requested.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    pub url: CrawlUrl,
    pub status: StatusCode,
    /// The `Content-Length` the server declared, read straight from the header.
    ///
    /// `None` when it declared none — which is *unknown*, not zero. A rule
    /// about size has to be able to say it does not know.
    pub content_length: Option<u64>,
    /// The media type alone, lowercased, as `Fetched::mime` reports it.
    pub content_type: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("robots.txt disallows {0}")]
    RobotsDenied(CrawlUrl),
    /// robots.txt could not be read, so RFC 9309 forbids the whole origin.
    ///
    /// Separate from `RobotsDenied` because they mean opposite things to
    /// whoever reads the report: one is a site that banned us and must be
    /// respected, the other is usually a site that is down and should be
    /// retried. Collapsing them was T1.6a.
    #[error(
        "robots.txt for {url} could not be read ({reason}), so nothing on the origin may be fetched"
    )]
    RobotsUnreadable { url: CrawlUrl, reason: String },
    #[error("no response from {url} after {attempts} attempt(s): {reason}")]
    Transport {
        url: CrawlUrl,
        attempts: u32,
        reason: String,
    },
}

pub struct Fetcher {
    client: Client,
    robots: RobotsCache,
    limiter: Limiter,
    retry: RetryPolicy,
    pub(crate) config: FetchConfig,
}

impl Fetcher {
    pub fn new(config: FetchConfig) -> reqwest::Result<Self> {
        Ok(Self {
            client: crate::client()?,
            robots: RobotsCache::new(crate::PRODUCT_TOKEN),
            limiter: Limiter::new(config.max_concurrent_per_host),
            retry: RetryPolicy::default(),
            config,
        })
    }

    /// Fetches exactly one URL. Redirects come back as themselves — walking the
    /// chain is the caller's job, because the chain is data.
    pub async fn fetch(&self, url: &CrawlUrl) -> Result<Fetched, FetchError> {
        let (resp, started, permit) = self.send(url, Method::GET).await?;
        let out = self.read(url, resp, started).await;
        // Held until here on purpose: the host's concurrency permit has to
        // cover the body as well as the request. A permit released before the
        // body is drained is not a concurrency limit.
        drop(permit);
        out
    }

    /// Checks one URL with `HEAD`: status and headers, never a body.
    ///
    /// Same robots.txt, same per-host limiter, same retry policy as `fetch` —
    /// it is literally the same code path with a different method, because a
    /// second request path that skipped any of those would be a hole in the
    /// politeness guarantee rather than a shortcut.
    pub async fn head(&self, url: &CrawlUrl) -> Result<Head, FetchError> {
        let (resp, _started, _permit) = self.send(url, Method::HEAD).await?;
        Ok(Head {
            url: url.clone(),
            status: resp.status(),
            // Read from the header rather than from `Response::content_length`.
            // A HEAD response has no body, so the body's size hint describes
            // the absence rather than the resource, and reporting 0 for a
            // 4 MB image would make `oversized-image` silently blind.
            content_length: resp
                .headers()
                .get(CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse().ok()),
            content_type: mime_of(resp.headers()),
        })
    }

    /// The politeness path, shared by every request Pounce makes.
    ///
    /// Returns the permit alongside the response so the caller decides when the
    /// request is really over — for a `GET` that is after the body, for a
    /// `HEAD` there is no body to wait for.
    async fn send(
        &self,
        url: &CrawlUrl,
        method: Method,
    ) -> Result<(reqwest::Response, Instant, OwnedSemaphorePermit), FetchError> {
        match self.robots.access(&self.client, url).await {
            Access::Allowed => {}
            Access::Disallowed => return Err(FetchError::RobotsDenied(url.clone())),
            Access::Unreadable(reason) => {
                return Err(FetchError::RobotsUnreadable {
                    url: url.clone(),
                    reason,
                });
            }
        }

        // Whatever this host asked for, or our default if it asked for nothing.
        let delay = self
            .robots
            .get(&self.client, url)
            .await
            .crawl_delay()
            .unwrap_or(self.config.default_delay);

        let mut attempt = 1;
        loop {
            // Inside the loop deliberately: a retry is another request to a
            // host that just told us it was struggling. Backing off without
            // re-entering the limiter turns a retry storm into a burst.
            let permit = self.limiter.acquire(url.host(), delay).await;

            let started = Instant::now();
            let sent = self
                .client
                .request(method.clone(), url.as_url().clone())
                .timeout(self.config.timeout)
                .send()
                .await;

            let wait = match &sent {
                Ok(resp) => self
                    .retry
                    .after_status(attempt, resp.status(), resp.headers()),
                Err(_) => self.retry.after_transport_error(attempt),
            };

            if let Some(wait) = wait {
                // Released before sleeping: waiting out a backoff is not an
                // open connection.
                drop(permit);
                tokio::time::sleep(wait).await;
                attempt += 1;
                continue;
            }

            return match sent {
                Ok(resp) => Ok((resp, started, permit)),
                Err(e) => Err(FetchError::Transport {
                    url: url.clone(),
                    attempts: attempt,
                    reason: e.to_string(),
                }),
            };
        }
    }

    async fn read(
        &self,
        url: &CrawlUrl,
        mut resp: reqwest::Response,
        started: Instant,
    ) -> Result<Fetched, FetchError> {
        let status = resp.status();
        let headers = resp.headers().clone();
        // Taken before the body is touched: everything after this point is
        // transfer time, not server think time.
        let time_to_headers = started.elapsed();
        let declared_length = resp.content_length();

        let mut body = Vec::new();
        let mut truncated = false;
        loop {
            match resp.chunk().await {
                Ok(Some(chunk)) => {
                    let room = self.config.max_body_bytes - body.len();
                    if chunk.len() >= room {
                        body.extend_from_slice(&chunk[..room]);
                        truncated = true;
                        break;
                    }
                    body.extend_from_slice(&chunk);
                }
                Ok(None) => break,
                // The status and headers arrived, but the body did not. That is
                // a failed fetch, not a short page.
                Err(e) => {
                    return Err(FetchError::Transport {
                        url: url.clone(),
                        attempts: 1,
                        reason: e.to_string(),
                    });
                }
            }
        }

        Ok(Fetched {
            url: url.clone(),
            status,
            headers,
            body,
            truncated,
            elapsed: started.elapsed(),
            time_to_headers,
            declared_length,
        })
    }
}
