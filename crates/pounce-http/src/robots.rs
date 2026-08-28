//! robots.txt: fetch, parse, cache.
//!
//! Parsing and matching come from `robotxt`, which implements RFC 9309
//! including the parts that are easy to get subtly wrong — longest-match
//! `Allow` precedence, `*` and `$` wildcards, and user-agent group selection.
//! What lives here is the crawler-side half: turning an HTTP response into the
//! RFC's access semantics, and holding one answer per origin.
//!
//! The status code decides policy before any byte is parsed. A 4xx means no
//! file, which permits everything; a 5xx or a connection failure means the
//! file is *undefined*, which forbids everything. Reading those the same way
//! is how a crawler hammers a site that was trying to tell it to stop.

use pounce_core::CrawlUrl;
use reqwest::{Client, StatusCode, header};
use robotxt::{AccessResult, BYTE_LIMIT};
use std::collections::HashMap;
use std::sync::Mutex;
use url::Url;

pub use robotxt::Robots;

/// Whether a URL may be fetched, and if not, why not.
///
/// The distinction between the two refusals is the whole point of this type.
/// RFC 9309 makes an unreadable robots.txt a complete disallow, so a host that
/// is simply *down* produces exactly the same verdict as a host that has
/// deliberately banned us. That is correct as behaviour and useless as a crawl
/// report: "the site forbids this" and "the site is offline" need opposite
/// responses from whoever reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Access {
    Allowed,
    /// A rule in a robots.txt we successfully read matched this URL.
    Disallowed,
    /// robots.txt could not be established, so nothing on the origin may be
    /// fetched. Carries a short human reason — a failed connection, or the
    /// status that made the file undefined.
    Unreadable(String),
}

/// RFC 9309: crawlers SHOULD follow at least five consecutive redirects.
const MAX_REDIRECTS: usize = 5;

/// One parsed robots.txt per origin, fetched on first use.
///
/// Keyed by origin rather than host because `http://` and `https://` on the
/// same name are separate documents and may disagree.
pub struct RobotsCache {
    /// The product token to match against `User-agent:` lines — `PounceBot`,
    /// not the full header string.
    agent: String,
    origins: Mutex<HashMap<String, Entry>>,
}

/// A cached origin: the rules, plus why they had to be assumed if they did.
#[derive(Clone)]
struct Entry {
    rules: Robots,
    /// `Some(reason)` when the file could not be read and the rules are the
    /// RFC's fallback rather than the site's own.
    unreadable: Option<String>,
    /// The file as served, kept so the audit can *show* robots.txt rather than
    /// describe it. Every audit opens with this file, and reading it for
    /// politeness while never reporting it was the gap.
    body: Option<String>,
    /// The status the file answered with. 0 when the request produced no
    /// response at all, which is a different report from a 404.
    status: u16,
}

impl RobotsCache {
    pub fn new(agent: impl Into<String>) -> Self {
        Self {
            agent: agent.into(),
            origins: Mutex::new(HashMap::new()),
        }
    }

    /// The rules governing `u`'s origin, fetching them if this is the first
    /// URL seen there. Also the way to reach `crawl_delay`.
    pub async fn get(&self, client: &Client, u: &CrawlUrl) -> Robots {
        self.entry(client, u).await.rules
    }

    async fn entry(&self, client: &Client, u: &CrawlUrl) -> Entry {
        let origin = u.as_url().origin().ascii_serialization();
        let cached = self.origins.lock().unwrap().get(&origin).cloned();
        if let Some(entry) = cached {
            return entry;
        }

        // CrawlUrl already guarantees an http(s) URL with a host, which is
        // everything create_url rejects; the fallback is unreachable.
        let robots_url = match robotxt::create_url(u.as_url()) {
            Ok(url) => url,
            Err(_) => {
                return Entry {
                    rules: Robots::from_always(true, &self.agent),
                    unreadable: None,
                    body: None,
                    status: 0,
                };
            }
        };

        // ponytail: two tasks reaching a new origin together both fetch, and
        // the second insert wins. Costs one duplicate request per origin at
        // worst. Worth a per-origin lock only if it shows up in a profile.
        let entry = fetch(client, robots_url, &self.agent).await;
        self.origins.lock().unwrap().insert(origin, entry.clone());
        entry
    }

    /// The verdict for one URL, keeping the reason a refusal happened.
    pub async fn access(&self, client: &Client, u: &CrawlUrl) -> Access {
        let entry = self.entry(client, u).await;
        if entry.rules.is_absolute_allowed(u.as_url()) {
            return Access::Allowed;
        }
        match entry.unreadable {
            Some(reason) => Access::Unreadable(reason),
            None => Access::Disallowed,
        }
    }

    pub async fn is_allowed(&self, client: &Client, u: &CrawlUrl) -> bool {
        self.access(client, u).await == Access::Allowed
    }

    /// Every robots.txt this crawl read: origin, status, and the file itself.
    ///
    /// For the report rather than for the crawl. The politeness path has
    /// always had this and thrown it away, which is why an audit could not
    /// show the first file any auditor opens.
    pub fn fetched(&self) -> Vec<(String, u16, Option<String>)> {
        let origins = self.origins.lock().unwrap();
        let mut out = origins
            .iter()
            .map(|(origin, e)| (origin.clone(), e.status, e.body.clone()))
            .collect::<Vec<_>>();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// The sitemaps robots.txt declares for this URL's origin.
    ///
    /// `Sitemap:` is an extension the RFC does not require and every real site
    /// uses. `robotxt` already parses it; nothing had asked.
    pub async fn sitemaps(&self, client: &Client, u: &CrawlUrl) -> Vec<Url> {
        self.get(client, u).await.sitemaps().to_vec()
    }
}

/// A short, stable reason for a request that produced no response.
///
/// `reqwest`'s `Display` includes the full URL and a chain of causes, which is
/// noise in a crawl report where the URL is already the row you are looking at.
fn transport_reason(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "connection timed out".into()
    } else if e.is_connect() {
        "could not connect".into()
    } else {
        "request failed".into()
    }
}

/// Maps a settled (non-redirect) response to RFC 9309 §2.3.1 access semantics.
fn access(status: StatusCode, body: &[u8]) -> AccessResult<'_> {
    if status.is_success() {
        AccessResult::Successful(body)
    } else if status.is_redirection() {
        AccessResult::Redirect
    } else if status.is_server_error() {
        AccessResult::Unreachable
    } else {
        AccessResult::Unavailable
    }
}

async fn fetch(client: &Client, robots_url: Url, agent: &str) -> Entry {
    let unreachable = |reason: String| Entry {
        rules: Robots::from_access(AccessResult::Unreachable, agent),
        unreadable: Some(reason),
        body: None,
        status: 0,
    };
    let mut next = robots_url;

    // Auto-redirect is disabled client-wide, so the hops are walked here.
    for _ in 0..=MAX_REDIRECTS {
        let mut resp = match client.get(next.clone()).send().await {
            Ok(resp) => resp,
            Err(e) => return unreachable(transport_reason(&e)),
        };

        let status = resp.status();
        if status.is_redirection() {
            let target = resp
                .headers()
                .get(header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|loc| next.join(loc).ok());
            match target {
                Some(url) => {
                    next = url;
                    continue;
                }
                // A redirect with no usable Location is a broken file, not a
                // ban: the RFC's "unavailable" permits everything.
                None => {
                    return Entry {
                        rules: Robots::from_access(AccessResult::Unavailable, agent),
                        unreadable: None,
                        body: None,
                        status: status.as_u16(),
                    };
                }
            }
        }

        let mut body = Vec::new();
        if status.is_success() {
            // Read to the size limit rather than buffering whatever the
            // server sends: this is untrusted input on an unauthenticated
            // path, and a truncated read must not be parsed as complete.
            loop {
                match resp.chunk().await {
                    Ok(Some(chunk)) => {
                        let room = BYTE_LIMIT - body.len();
                        if chunk.len() >= room {
                            body.extend_from_slice(&chunk[..room]);
                            break;
                        }
                        body.extend_from_slice(&chunk);
                    }
                    Ok(None) => break,
                    Err(e) => return unreachable(transport_reason(&e)),
                }
            }
        }
        // Only a 5xx leaves the rules undefined. A 4xx means "no such file",
        // which permits everything, so it is not an unreadable origin.
        return Entry {
            rules: Robots::from_access(access(status, &body), agent),
            unreadable: status
                .is_server_error()
                .then(|| format!("robots.txt returned HTTP {}", status.as_u16())),
            // Lossy: robots.txt is specified as UTF-8, and a file that is not
            // is still worth showing with its bad bytes marked rather than
            // reported as unreadable.
            body: status
                .is_success()
                .then(|| String::from_utf8_lossy(&body).into_owned()),
            status: status.as_u16(),
        };
    }

    // More than five hops: the RFC permits treating the file as unavailable.
    Entry {
        rules: Robots::from_access(AccessResult::Redirect, agent),
        unreadable: None,
        body: None,
        status: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const AGENT: &str = "PounceBot";

    fn rules(txt: &str) -> Robots {
        Robots::from_bytes(txt.as_bytes(), AGENT)
    }

    fn allowed(txt: &str, path: &str) -> bool {
        rules(txt).is_relative_allowed(path)
    }

    // ---- access semantics (RFC 9309 §2.3.1) ----
    //
    // The status code decides the policy before a single byte is parsed, and
    // getting this backwards is how a crawler ends up hammering a site that
    // was trying to tell it to stop.

    #[test]
    fn a_2xx_body_is_the_rules() {
        let r = Robots::from_access(
            access(StatusCode::OK, b"User-agent: *\nDisallow: /x"),
            AGENT,
        );
        assert!(!r.is_relative_allowed("/x"));
        assert!(r.is_relative_allowed("/y"));
    }

    #[test]
    fn a_4xx_allows_everything() {
        // No robots.txt is not a prohibition. This is the common case: most
        // sites 404 here.
        let r = Robots::from_access(access(StatusCode::NOT_FOUND, b""), AGENT);
        assert!(r.is_relative_allowed("/anything"));
    }

    #[test]
    fn a_5xx_disallows_everything() {
        // The file is undefined rather than absent, so the polite reading is
        // that we do not know what we are allowed to do.
        let r = Robots::from_access(access(StatusCode::INTERNAL_SERVER_ERROR, b""), AGENT);
        assert!(!r.is_relative_allowed("/anything"));
    }

    #[test]
    fn a_body_on_an_error_status_is_ignored() {
        // A 500 page that happens to contain "Allow: /" is not permission.
        let body = b"User-agent: *\nAllow: /";
        let r = Robots::from_access(access(StatusCode::SERVICE_UNAVAILABLE, body), AGENT);
        assert!(!r.is_relative_allowed("/anything"));
    }

    // ---- allow precedence ----

    #[test]
    fn the_longest_matching_rule_wins() {
        let txt = "User-agent: *\nDisallow: /admin/\nAllow: /admin/public/";
        assert!(!allowed(txt, "/admin/secret"));
        assert!(allowed(txt, "/admin/public/page"));
    }

    #[test]
    fn allow_wins_a_tie_with_disallow() {
        let txt = "User-agent: *\nDisallow: /p\nAllow: /p";
        assert!(allowed(txt, "/p"));
    }

    #[test]
    fn an_empty_disallow_is_not_a_block() {
        let txt = "User-agent: *\nDisallow:";
        assert!(allowed(txt, "/anything"));
    }

    #[test]
    fn disallow_root_blocks_the_whole_site() {
        let txt = "User-agent: *\nDisallow: /";
        assert!(!allowed(txt, "/"));
        assert!(!allowed(txt, "/deep/page"));
    }

    // ---- wildcards ----

    #[test]
    fn a_star_matches_any_run_of_characters() {
        let txt = "User-agent: *\nDisallow: /*.pdf";
        assert!(!allowed(txt, "/docs/report.pdf"));
        assert!(allowed(txt, "/docs/report.html"));
    }

    #[test]
    fn a_dollar_anchors_the_end() {
        let txt = "User-agent: *\nDisallow: /*.php$";
        assert!(!allowed(txt, "/index.php"));
        assert!(allowed(txt, "/index.php?q=1"));
    }

    #[test]
    fn a_prefix_rule_matches_query_strings_too() {
        let txt = "User-agent: *\nDisallow: /search";
        assert!(!allowed(txt, "/search?q=shoes"));
        assert!(!allowed(txt, "/searchresults"));
    }

    // ---- user-agent group selection ----

    #[test]
    fn our_own_group_beats_the_wildcard_group() {
        let txt = "User-agent: *\nDisallow: /\n\nUser-agent: PounceBot\nDisallow: /private/";
        assert!(allowed(txt, "/public"));
        assert!(!allowed(txt, "/private/x"));
    }

    #[test]
    fn another_bots_group_does_not_apply_to_us() {
        let txt = "User-agent: Googlebot\nDisallow: /\n\nUser-agent: *\nDisallow: /admin/";
        assert!(allowed(txt, "/public"));
        assert!(!allowed(txt, "/admin/x"));
    }

    #[test]
    fn user_agent_matching_ignores_case() {
        let txt = "User-agent: pouncebot\nDisallow: /x";
        assert!(!allowed(txt, "/x"));
    }

    // ---- crawl-delay ----

    #[test]
    fn reads_crawl_delay_for_our_group() {
        let txt = "User-agent: PounceBot\nCrawl-delay: 2\nDisallow:";
        assert_eq!(rules(txt).crawl_delay(), Some(Duration::from_secs(2)));
    }

    #[test]
    fn crawl_delay_is_absent_when_not_stated() {
        assert_eq!(rules("User-agent: *\nDisallow: /x").crawl_delay(), None);
    }

    // ---- malformed input ----
    //
    // robots.txt is written by hand and served by anything. None of this may
    // panic, and none of it may be read as a blanket block.

    #[test]
    fn survives_junk_without_blocking_the_crawl() {
        for txt in [
            "",
            "\n\n\n",
            "not a directive at all",
            "Disallow: /orphan-rule-before-any-user-agent",
            "User-agent:\nDisallow: /x",
            "User-agent: *\nDisallow /missing-colon",
            "User-agent: *\nCrawl-delay: soon\nDisallow: /x",
            "<!DOCTYPE html><html><body>404</body></html>",
            "User-agent: *\r\nDisallow: /x\r\n",
        ] {
            let r = rules(txt);
            // The call is what matters: it must return rather than panic.
            let _ = r.is_relative_allowed("/some/page");
        }
    }

    #[test]
    fn an_html_error_page_served_as_robots_txt_allows_the_crawl() {
        // Servers that return a styled 200 error page here are common, and
        // treating that as a block would silently stop the crawl.
        let txt = "<!DOCTYPE html><html><body><h1>Not found</h1></body></html>";
        assert!(allowed(txt, "/page/1"));
    }

    #[test]
    fn comments_are_ignored() {
        let txt = "# a comment\nUser-agent: *  # trailing\nDisallow: /x\n";
        assert!(!allowed(txt, "/x"));
        assert!(allowed(txt, "/y"));
    }
}
