//! Walking the redirect chain by hand.
//!
//! `reqwest`'s auto-redirect is disabled workspace-wide, so this is where a
//! `301` becomes a second request. That is deliberate: the chain is the finding.
//! A crawler that transparently follows redirects can report the destination
//! but not the four hops, the mixed 302/301 statuses, or the cross-host jump in
//! the middle — which is most of what a redirect audit is actually about.
//!
//! The walk always returns a chain. A loop, a hop-limit blowout, a missing
//! `Location`, and a robots-denied hop are all *outcomes* rather than errors,
//! because every one of them is something the report must show next to the
//! hops that led there. Returning `Err` would throw away the evidence.

use crate::fetch::{FetchError, Fetched, Fetcher};
use pounce_core::CrawlUrl;
use reqwest::StatusCode;
use reqwest::header::LOCATION;
use std::collections::HashSet;

/// One redirect response in the chain.
#[derive(Debug, Clone)]
pub struct Hop {
    pub url: CrawlUrl,
    pub status: StatusCode,
    /// The raw `Location` header, kept verbatim. A relative or malformed value
    /// is worth reporting as the server sent it.
    pub location: String,
    /// `location` resolved against `url`; `None` when it would not parse.
    pub target: Option<CrawlUrl>,
}

/// How the walk ended.
#[derive(Debug)]
pub enum Outcome {
    /// A non-redirect response. This is the ordinary case, and `hops` is empty
    /// when the very first request landed.
    Landed(Fetched),
    /// A URL already visited in this chain. Carries the URL that repeated.
    Loop(CrawlUrl),
    /// Still redirecting after `max_redirects` hops.
    HopLimit,
    /// A redirect status with no usable `Location` — absent, empty, non-ASCII,
    /// or not resolvable against the current URL.
    NoLocation,
    /// A hop could not be fetched at all: robots.txt, or transport failure.
    Failed(FetchError),
}

/// The full walk: every redirect crossed, and how it ended.
#[derive(Debug)]
pub struct RedirectChain {
    pub start: CrawlUrl,
    pub hops: Vec<Hop>,
    pub outcome: Outcome,
}

impl RedirectChain {
    /// The URL the walk finished on — the landing page, or the last URL tried.
    pub fn final_url(&self) -> &CrawlUrl {
        match &self.outcome {
            Outcome::Landed(f) => &f.url,
            _ => self
                .hops
                .last()
                .and_then(|h| h.target.as_ref())
                .unwrap_or(&self.start),
        }
    }

    /// True when the chain ended in something an audit should flag rather than
    /// a page it should record.
    pub fn is_broken(&self) -> bool {
        !matches!(self.outcome, Outcome::Landed(_))
    }
}

impl Fetcher {
    /// Fetches `url`, following redirects up to the configured hop cap and
    /// recording each one.
    pub async fn follow(&self, url: &CrawlUrl) -> RedirectChain {
        let mut hops: Vec<Hop> = Vec::new();
        let mut seen: HashSet<CrawlUrl> = HashSet::from([url.clone()]);
        let mut current = url.clone();

        loop {
            let fetched = match self.fetch(&current).await {
                Ok(f) => f,
                // Every hop already crossed stays in the chain. Losing them
                // here is exactly the reporting gap T1.6a is about: "blocked"
                // is only actionable next to the route that reached the block.
                Err(e) => return ended(url, hops, Outcome::Failed(e)),
            };

            if !is_redirect(fetched.status) {
                return ended(url, hops, Outcome::Landed(fetched));
            }

            // Checked before recording, so `max_redirects` counts hops in the
            // chain rather than requests sent. One request beyond the cap is
            // unavoidable — a response has to arrive before it can be known to
            // be a redirect — but it must not become a recorded hop, or a
            // chain of exactly `max_redirects` would be reported as overlong.
            if hops.len() >= self.config.max_redirects {
                return ended(url, hops, Outcome::HopLimit);
            }

            let location = fetched
                .headers
                .get(LOCATION)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .to_string();
            // `join` also rejects schemes we cannot crawl, which is what turns
            // a `Location: mailto:…` into a reported dead end rather than a
            // parse error thrown away at the call site. The empty case is
            // excluded explicitly: joining "" succeeds and yields the current
            // URL, so an absent or unreadable `Location` would otherwise be
            // reported as a redirect loop instead of a malformed redirect.
            let target = if location.is_empty() {
                None
            } else {
                current.join(&location).ok()
            };

            hops.push(Hop {
                url: current,
                status: fetched.status,
                location,
                target: target.clone(),
            });

            let Some(target) = target else {
                return ended(url, hops, Outcome::NoLocation);
            };

            // Before the hop cap on purpose: a three-hop loop under a ten-hop
            // budget is a loop, and reporting it as a long chain would hide
            // the only detail that makes it fixable.
            if !seen.insert(target.clone()) {
                return ended(url, hops, Outcome::Loop(target));
            }

            current = target;
        }
    }
}

fn ended(start: &CrawlUrl, hops: Vec<Hop>, outcome: Outcome) -> RedirectChain {
    RedirectChain {
        start: start.clone(),
        hops,
        outcome,
    }
}

/// Whether a status means "go elsewhere". `304 Not Modified` is deliberately
/// absent: it carries no `Location` and is a cache answer, not a redirect.
pub fn is_redirect(status: StatusCode) -> bool {
    // Listed rather than derived from `is_redirection`, which also covers 300
    // Multiple Choices and 305 Use Proxy — neither of which carries a
    // `Location` we should chase.
    matches!(status.as_u16(), 301 | 302 | 303 | 307 | 308)
}
