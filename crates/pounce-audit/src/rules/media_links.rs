//! The Media & links batch: images that say nothing, links that go nowhere,
//! and pages nothing points at.

use crate::issue::{Issue, RuleMeta, Severity};
use crate::registry::{Registry, RegistryError};
use crate::rule::{PageRule, SiteRule};
use pounce_parse::PageRecord;
use pounce_store::{Store, StoreError};

pub fn register(registry: &mut Registry) -> Result<(), RegistryError> {
    registry.register_page(Box::new(MissingAlt))?;
    registry.register_site(Box::new(BrokenInternalLink))?;
    registry.register_site(Box::new(OrphanPage))?;
    Ok(())
}

pub struct MissingAlt;

impl PageRule for MissingAlt {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "media.missing-alt",
            severity: Severity::Warning,
            description: "The page has images with no alt attribute at all.",
            remediation: "Describe what the image shows. If it is decorative, write alt=\"\" — an empty alt is a decision, an absent one is an omission.",
        }
    }
    fn check(&self, page: &PageRecord, out: &mut Vec<Issue>) {
        // `None` only. `Some("")` is the documented decorative marker and is
        // the correct fix, not a defect — the same absent-versus-empty split
        // the record keeps everywhere.
        let missing = page.images.iter().filter(|i| i.alt.is_none()).count();
        if missing == 0 {
            return;
        }
        // One issue per page rather than per image: a template that forgot
        // `alt` produces one defect repeated, and a row per image would bury
        // every other finding on a page with fifty thumbnails.
        let first = page
            .images
            .iter()
            .find(|i| i.alt.is_none())
            .map(|i| i.src.as_str())
            .unwrap_or_default();
        out.push(Issue {
            rule_id: self.meta().id,
            severity: self.meta().severity,
            detail: Some(format!(
                "{missing} of {} images: {first}",
                page.images.len()
            )),
        });
    }
}

pub struct BrokenInternalLink;

impl SiteRule for BrokenInternalLink {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "links.broken-internal",
            severity: Severity::Warning,
            description: "Pages on this site link to a URL that does not return a usable page.",
            remediation: "Point the links at a working URL, or restore the destination.",
        }
    }
    fn check(&self, store: &Store) -> Result<Vec<(String, Issue)>, StoreError> {
        // A target with no `pages` row is *uncrawled*, not broken — most of
        // them are external. Only a row that came back >= 400 is a finding, so
        // the join is what scopes this to internal links without needing to
        // re-derive the crawl scope here.
        //
        // Terminal failures are deliberately not included. `crawl_failures`
        // holds robots denials, which `indexability.blocked-but-linked`
        // already reports against the same URL, and reporting both would
        // charge one site defect twice.
        let mut stmt = store.conn().prepare(
            "SELECT p.url, p.status, count(DISTINCT l.source_page_id) FROM pages p \
             JOIN links l ON l.target_url = p.url \
             WHERE p.status >= 400 \
             GROUP BY p.url, p.status ORDER BY p.url",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (url, status, sources) = row?;
            out.push((
                url,
                Issue {
                    rule_id: self.meta().id,
                    severity: self.meta().severity,
                    detail: Some(format!("{status}, linked from {sources} page(s)")),
                },
            ));
        }
        Ok(out)
    }
}

pub struct OrphanPage;

impl SiteRule for OrphanPage {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "links.orphan-page",
            severity: Severity::Notice,
            description: "The page was crawled but no other page links to it.",
            remediation: "Link to it from somewhere relevant, or confirm it is meant to be reachable only by people who already have the URL.",
        }
    }
    fn check(&self, store: &Store) -> Result<Vec<(String, Issue)>, StoreError> {
        // `depth = 0` is the URL the crawl was started from — and, when the
        // seed redirects, the page it landed on. Nothing on the site is
        // expected to link to it, so excluding it removes one guaranteed
        // false finding per crawl. Every other page was reached somehow, and
        // if no edge points at it that route was a redirect or a sitemap,
        // which is the finding.
        //
        // `nofollow` is not filtered: it is a ranking hint, not the absence of
        // a link, and a page reachable only through one is still reachable.
        let mut stmt = store.conn().prepare(
            "SELECT p.url FROM pages p \
             WHERE p.depth > 0 \
               AND NOT EXISTS (SELECT 1 FROM links l WHERE l.target_url = p.url) \
             ORDER BY p.url",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for url in rows {
            out.push((
                url?,
                Issue {
                    rule_id: self.meta().id,
                    severity: self.meta().severity,
                    detail: None,
                },
            ));
        }
        Ok(out)
    }
}
