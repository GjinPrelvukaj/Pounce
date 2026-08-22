//! Response-level findings: what the server said, and how it got there.

use crate::issue::{Issue, RuleMeta, Severity};
use crate::registry::{Registry, RegistryError};
use crate::rule::{PageRule, SiteRule};
use pounce_parse::PageRecord;
use pounce_store::{Store, StoreError};

/// Redirect hops beyond this are worth reporting.
///
/// Two is the point where a chain stops being a single tidy redirect and starts
/// costing crawl budget and link equity on every visit.
const MAX_CLEAN_HOPS: usize = 2;

pub fn register(registry: &mut Registry) -> Result<(), RegistryError> {
    registry.register_page(Box::new(ClientError))?;
    registry.register_page(Box::new(ServerError))?;
    registry.register_page(Box::new(LongRedirectChain))?;
    registry.register_page(Box::new(MixedContent))?;
    registry.register_site(Box::new(RedirectLoop))?;
    Ok(())
}

pub struct ClientError;

impl PageRule for ClientError {
    /// Applies to every resource, not just HTML: a 404 PDF is still a 404, and
    /// a sitemap reached through four redirects still costs four requests.
    fn applies(&self, _page: &PageRecord) -> bool {
        true
    }

    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "response.4xx",
            severity: Severity::Warning,
            description: "The page returned a client error, so visitors and search engines see nothing.",
            remediation: "Restore the page, or redirect it to the closest equivalent and fix the links pointing at it.",
        }
    }
    fn check(&self, page: &PageRecord, out: &mut Vec<Issue>) {
        if (400..500).contains(&page.status) {
            out.push(Issue {
                rule_id: self.meta().id,
                severity: self.meta().severity,
                detail: Some(format!("HTTP {}", page.status)),
            });
        }
    }
}

pub struct ServerError;

impl PageRule for ServerError {
    /// Applies to every resource, not just HTML: a 404 PDF is still a 404, and
    /// a sitemap reached through four redirects still costs four requests.
    fn applies(&self, _page: &PageRecord) -> bool {
        true
    }

    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "response.5xx",
            severity: Severity::Critical,
            // Critical rather than Warning: a 4xx is usually a decision someone
            // made, a 5xx is the site failing, and it often means more pages are
            // broken than the crawl happened to catch.
            description: "The server failed while returning this page.",
            remediation: "Check server logs for this URL. A 5xx during a crawl often means intermittent failures affecting real visitors too.",
        }
    }
    fn check(&self, page: &PageRecord, out: &mut Vec<Issue>) {
        if page.status >= 500 {
            out.push(Issue {
                rule_id: self.meta().id,
                severity: self.meta().severity,
                detail: Some(format!("HTTP {}", page.status)),
            });
        }
    }
}

pub struct LongRedirectChain;

impl PageRule for LongRedirectChain {
    /// Applies to every resource, not just HTML: a 404 PDF is still a 404, and
    /// a sitemap reached through four redirects still costs four requests.
    fn applies(&self, _page: &PageRecord) -> bool {
        true
    }

    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "response.redirect-chain",
            severity: Severity::Warning,
            description: "Reaching this page took more redirects than it should.",
            remediation: "Point the first URL straight at the final destination so each visit costs one request instead of several.",
        }
    }
    fn check(&self, page: &PageRecord, out: &mut Vec<Issue>) {
        let hops = page.redirect_chain.len();
        if hops > MAX_CLEAN_HOPS {
            out.push(Issue {
                rule_id: self.meta().id,
                severity: self.meta().severity,
                detail: Some(format!("{hops} redirects")),
            });
        }
    }
}

pub struct MixedContent;

impl PageRule for MixedContent {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "response.mixed-content",
            severity: Severity::Critical,
            description: "A secure page links to an insecure one.",
            remediation: "Update the link to https, or remove it if the destination has no secure version.",
        }
    }
    fn check(&self, page: &PageRecord, out: &mut Vec<Issue>) {
        if page.url.scheme() != "https" {
            return;
        }
        for link in &page.links {
            // Only resolved targets: an unparseable href is a different
            // finding, and guessing its scheme would invent one.
            if link.target.as_ref().is_some_and(|t| t.scheme() == "http") {
                out.push(Issue {
                    rule_id: self.meta().id,
                    severity: self.meta().severity,
                    detail: Some(link.href.clone()),
                });
            }
        }
    }
}

/// A redirect loop, read from the durable record rather than from a page.
///
/// This is a `SiteRule` for a structural reason, not a stylistic one: a loop
/// **never produces a page**, so there is no `PageRecord` for a `PageRule` to
/// see. The evidence lives in `crawl_redirects`, keyed by the source URL.
pub struct RedirectLoop;

impl SiteRule for RedirectLoop {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "response.redirect-loop",
            severity: Severity::Critical,
            description: "This URL redirects in a circle and never resolves.",
            remediation: "Find the rule that sends the last hop back to an earlier one. Nothing at this URL is reachable until it is removed.",
        }
    }
    fn check(&self, store: &Store) -> Result<Vec<(String, Issue)>, StoreError> {
        let mut stmt = store.conn().prepare(
            "SELECT source_url, chain FROM crawl_redirects \
             WHERE outcome LIKE 'redirect loop%' ORDER BY source_url",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (url, chain) = row?;
            // The hop count comes from the stored chain, so the report says how
            // tight the loop is rather than merely that one exists.
            let hops = serde_json::from_str::<serde_json::Value>(&chain)
                .ok()
                .and_then(|v| v.as_array().map(Vec::len));
            out.push((
                url,
                Issue {
                    rule_id: self.meta().id,
                    severity: self.meta().severity,
                    detail: hops.map(|n| format!("{n} hops")),
                },
            ));
        }
        Ok(out)
    }
}
