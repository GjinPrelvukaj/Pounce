//! Heading and body-content findings.

use crate::issue::{Issue, RuleMeta, Severity};
use crate::registry::{Registry, RegistryError};
use crate::rule::{PageRule, SiteRule};
use pounce_parse::PageRecord;
use pounce_store::{Store, StoreError};

/// Below this, a page is usually a stub, a filter permutation, or a template
/// with nothing in it.
const MIN_WORDS: u32 = 200;

pub fn register(registry: &mut Registry) -> Result<(), RegistryError> {
    registry.register_page(Box::new(MissingH1))?;
    registry.register_page(Box::new(MultipleH1))?;
    registry.register_page(Box::new(EmptyH1))?;
    registry.register_page(Box::new(ThinContent))?;
    registry.register_site(Box::new(DuplicateBody))?;
    Ok(())
}

pub struct MissingH1;

impl PageRule for MissingH1 {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "content.missing-h1",
            severity: Severity::Warning,
            description: "The page has no <h1>.",
            remediation: "Add one <h1> naming what the page is about. It is the heading a reader and a crawler both look for first.",
        }
    }
    fn check(&self, page: &PageRecord, out: &mut Vec<Issue>) {
        // Empty vec only. A present-but-blank `<h1></h1>` is `content.empty-h1`
        // — the same absent-versus-empty split the record keeps everywhere.
        if page.h1.is_empty() {
            out.push(Issue {
                rule_id: self.meta().id,
                severity: self.meta().severity,
                detail: None,
            });
        }
    }
}

pub struct MultipleH1;

impl PageRule for MultipleH1 {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "content.multiple-h1",
            severity: Severity::Notice,
            // Notice: HTML5 permits several, and it is a clarity problem
            // rather than a defect. Reporting it as a Warning next to a
            // missing H1 would flatten a real difference.
            description: "The page has more than one <h1>.",
            remediation: "Keep the one that names the page and demote the rest to <h2>, so the page has a single obvious subject.",
        }
    }
    fn check(&self, page: &PageRecord, out: &mut Vec<Issue>) {
        if page.h1.len() > 1 {
            out.push(Issue {
                rule_id: self.meta().id,
                severity: self.meta().severity,
                detail: Some(format!("{} <h1> elements", page.h1.len())),
            });
        }
    }
}

pub struct EmptyH1;

impl PageRule for EmptyH1 {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "content.empty-h1",
            severity: Severity::Warning,
            description: "The page has an <h1> with no text in it.",
            remediation: "Put the page's subject inside it, or remove it. An empty heading is markup pretending to be structure.",
        }
    }
    fn check(&self, page: &PageRecord, out: &mut Vec<Issue>) {
        if page.h1.iter().any(|h| h.is_empty()) {
            out.push(Issue {
                rule_id: self.meta().id,
                severity: self.meta().severity,
                detail: None,
            });
        }
    }
}

pub struct ThinContent;

impl PageRule for ThinContent {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "content.thin",
            severity: Severity::Warning,
            description: "The page has very little text on it.",
            remediation: "Add substance, merge it into a fuller page, or keep it out of the index if it exists for navigation.",
        }
    }
    fn check(&self, page: &PageRecord, out: &mut Vec<Issue>) {
        if page.word_count < MIN_WORDS {
            out.push(Issue {
                rule_id: self.meta().id,
                severity: self.meta().severity,
                detail: Some(format!("{} words", page.word_count)),
            });
        }
    }
}

/// Pages whose body text is identical to another page's.
///
/// Compares `body_hash` rather than text: the hash is computed once as the page
/// streams past and the bodies are never retained, which is what keeps memory
/// flat on a 500k crawl.
pub struct DuplicateBody;

impl SiteRule for DuplicateBody {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "content.duplicate-body",
            severity: Severity::Warning,
            description: "Another page has exactly the same body text.",
            remediation: "Keep one and redirect or canonicalise the rest, so the versions stop competing with each other.",
        }
    }
    fn check(&self, store: &Store) -> Result<Vec<(String, Issue)>, StoreError> {
        // NULL excluded on both sides: a page with no text has nothing to
        // compare, and treating those as matching would make every empty page a
        // duplicate of every other. The outer guard is belt-and-braces —
        // `NULL IN (...)` is never true in SQL, so it changes no result — but
        // it states the intent where a reader will look for it rather than
        // leaving the behaviour resting on a subtlety of three-valued logic.
        let mut stmt = store.conn().prepare(
            "SELECT url FROM pages \
             WHERE body_hash IS NOT NULL AND body_hash IN ( \
               SELECT body_hash FROM pages WHERE body_hash IS NOT NULL \
               GROUP BY body_hash HAVING count(*) > 1) \
             ORDER BY url",
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
