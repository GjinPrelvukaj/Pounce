//! Whether a page can be indexed, and whether it agrees with itself about it.

use crate::issue::{Issue, RuleMeta, Severity};
use crate::registry::{Registry, RegistryError};
use crate::rule::{PageRule, SiteRule};
use pounce_parse::PageRecord;
use pounce_store::{Store, StoreError};

/// The prefix `FetchError::RobotsDenied`'s `Display` produces.
///
/// Matching a message is fragile: change the wording and this rule stops
/// firing **silently**, which is the worst way for an audit rule to fail. The
/// coupling is therefore pinned by a test that builds the real error and
/// asserts this prefix still matches it, so the wording cannot drift unnoticed.
pub const ROBOTS_DENIED_PREFIX: &str = "robots.txt disallows";

pub fn register(registry: &mut Registry) -> Result<(), RegistryError> {
    registry.register_page(Box::new(Noindex))?;
    registry.register_page(Box::new(CanonicalMismatch))?;
    registry.register_site(Box::new(CanonicalToNon200))?;
    registry.register_site(Box::new(CanonicalChain))?;
    registry.register_site(Box::new(BlockedButLinked))?;
    Ok(())
}

pub struct Noindex;

impl PageRule for Noindex {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "indexability.noindex",
            severity: Severity::Warning,
            // Warning rather than Critical. The spec reserves Critical for
            // "noindex on important pages", and nothing here knows which pages
            // are important — a staging page carrying noindex is working as
            // intended. Rating every one Critical would train the reader to
            // ignore the colour.
            description: "The page tells search engines not to index it.",
            remediation: "Remove the noindex directive if this page should appear in results. If it should not, no action is needed.",
        }
    }
    fn check(&self, page: &PageRecord, out: &mut Vec<Issue>) {
        if page.meta_robots.noindex {
            out.push(Issue {
                rule_id: self.meta().id,
                severity: self.meta().severity,
                detail: None,
            });
        }
    }
}

pub struct CanonicalMismatch;

impl PageRule for CanonicalMismatch {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "indexability.canonical-elsewhere",
            severity: Severity::Notice,
            // Notice: pointing a duplicate at its original is the correct use
            // of a canonical. This is reported so the set is reviewable, not
            // because it is wrong.
            description: "The page's canonical points at a different URL, so this one is not the version that will rank.",
            remediation: "Confirm this is deliberate. If this page should rank, point the canonical at itself.",
        }
    }
    fn check(&self, page: &PageRecord, out: &mut Vec<Issue>) {
        let Some(canonical) = page.canonical_url.as_ref() else {
            return;
        };
        if canonical != &page.url {
            out.push(Issue {
                rule_id: self.meta().id,
                severity: self.meta().severity,
                detail: Some(canonical.to_string()),
            });
        }
    }
}

pub struct CanonicalToNon200;

impl SiteRule for CanonicalToNon200 {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "indexability.canonical-non-200",
            severity: Severity::Critical,
            description: "The page's canonical points at a URL that does not return 200, so it names a version that cannot rank.",
            remediation: "Point the canonical at a URL that returns 200, or fix the destination.",
        }
    }
    fn check(&self, store: &Store) -> Result<Vec<(String, Issue)>, StoreError> {
        let mut stmt = store.conn().prepare(
            "SELECT a.url, a.canonical_url, b.status FROM pages a \
             JOIN pages b ON b.url = a.canonical_url \
             WHERE a.canonical_url IS NOT NULL AND b.status != 200 \
             ORDER BY a.url",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (url, canonical, status) = row?;
            out.push((
                url,
                Issue {
                    rule_id: self.meta().id,
                    severity: self.meta().severity,
                    detail: Some(format!("{canonical} returns {status}")),
                },
            ));
        }
        Ok(out)
    }
}

pub struct CanonicalChain;

impl SiteRule for CanonicalChain {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "indexability.canonical-chain",
            severity: Severity::Warning,
            description: "The page's canonical points at a page that canonicalises somewhere else again.",
            remediation: "Point the first canonical straight at the final version. Chained canonicals are frequently ignored altogether.",
        }
    }
    fn check(&self, store: &Store) -> Result<Vec<(String, Issue)>, StoreError> {
        // A chain needs both hops to be real redirections of authority, so
        // both pages must canonicalise away from themselves. A self-referencing
        // canonical in the middle terminates the chain and is correct.
        let mut stmt = store.conn().prepare(
            "SELECT a.url, b.url, b.canonical_url FROM pages a \
             JOIN pages b ON b.url = a.canonical_url \
             WHERE a.canonical_url IS NOT NULL AND a.canonical_url != a.url \
               AND b.canonical_url IS NOT NULL AND b.canonical_url != b.url \
             ORDER BY a.url",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (url, middle, end) = row?;
            out.push((
                url,
                Issue {
                    rule_id: self.meta().id,
                    severity: self.meta().severity,
                    detail: Some(format!("via {middle} to {end}")),
                },
            ));
        }
        Ok(out)
    }
}

pub struct BlockedButLinked;

impl SiteRule for BlockedButLinked {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "indexability.blocked-but-linked",
            severity: Severity::Warning,
            description: "Internal links point at a URL that robots.txt disallows, so the crawl budget spent reaching it is wasted.",
            remediation: "Either allow the URL in robots.txt or stop linking to it. Linking to a blocked page tells a crawler to go somewhere it may not.",
        }
    }
    fn check(&self, store: &Store) -> Result<Vec<(String, Issue)>, StoreError> {
        let mut stmt = store.conn().prepare(
            "SELECT f.url, count(DISTINCT l.source_page_id) FROM crawl_failures f \
             JOIN links l ON l.target_url = f.url \
             WHERE f.reason LIKE ?1 || '%' \
             GROUP BY f.url ORDER BY f.url",
        )?;
        let rows = stmt.query_map([ROBOTS_DENIED_PREFIX], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (url, sources) = row?;
            out.push((
                url,
                Issue {
                    rule_id: self.meta().id,
                    severity: self.meta().severity,
                    detail: Some(format!("linked from {sources} page(s)")),
                },
            ));
        }
        Ok(out)
    }
}
