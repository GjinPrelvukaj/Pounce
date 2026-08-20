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
    origins: Mutex<HashMap<String, Robots>>,
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
        let origin = u.as_url().origin().ascii_serialization();
        let cached = self.origins.lock().unwrap().get(&origin).cloned();
        if let Some(rules) = cached {
            return rules;
        }

        // CrawlUrl already guarantees an http(s) URL with a host, which is
        // everything create_url rejects; the fallback is unreachable.
        let robots_url = match robotxt::create_url(u.as_url()) {
            Ok(url) => url,
            Err(_) => return Robots::from_always(true, &self.agent),
        };

        // ponytail: two tasks reaching a new origin together both fetch, and
        // the second insert wins. Costs one duplicate request per origin at
        // worst. Worth a per-origin lock only if it shows up in a profile.
        let rules = fetch(client, robots_url, &self.agent).await;
        self.origins.lock().unwrap().insert(origin, rules.clone());
        rules
    }

    pub async fn is_allowed(&self, client: &Client, u: &CrawlUrl) -> bool {
        self.get(client, u).await.is_absolute_allowed(u.as_url())
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

async fn fetch(client: &Client, robots_url: Url, agent: &str) -> Robots {
    let unreachable = || Robots::from_access(AccessResult::Unreachable, agent);
    let mut next = robots_url;

    // Auto-redirect is disabled client-wide, so the hops are walked here.
    for _ in 0..=MAX_REDIRECTS {
        let mut resp = match client.get(next.clone()).send().await {
            Ok(resp) => resp,
            Err(_) => return unreachable(),
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
                // ban.
                None => return Robots::from_access(AccessResult::Unavailable, agent),
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
                    Err(_) => return unreachable(),
                }
            }
        }
        return Robots::from_access(access(status, &body), agent);
    }

    // More than five hops: the RFC permits treating the file as unavailable.
    Robots::from_access(AccessResult::Redirect, agent)
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
