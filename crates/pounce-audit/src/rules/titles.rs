//! Title findings.
//!
//! Lengths are counted in **characters, not bytes**. `"Café"` is four
//! characters and five bytes, and a byte count would report accented or
//! non-Latin titles as longer than they are — quietly worst for exactly the
//! sites least likely to notice.

use crate::issue::{Issue, RuleMeta, Severity};
use crate::registry::{Registry, RegistryError};
use crate::rule::{PageRule, SiteRule};
use pounce_parse::PageRecord;
use pounce_store::{Store, StoreError};

/// Above this, search results truncate the title.
const MAX_CHARS: usize = 60;
/// Below this, a title is usually leaving description unused rather than being
/// deliberately terse.
const MIN_CHARS: usize = 30;

pub fn register(registry: &mut Registry) -> Result<(), RegistryError> {
    registry.register_page(Box::new(MissingTitle))?;
    registry.register_page(Box::new(TitleTooLong))?;
    registry.register_page(Box::new(TitleTooShort))?;
    registry.register_page(Box::new(MultipleTitles))?;
    registry.register_site(Box::new(DuplicateTitle))?;
    Ok(())
}

pub struct MissingTitle;

impl PageRule for MissingTitle {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "title.missing",
            severity: Severity::Critical,
            description: "The page has no <title> element.",
            remediation: "Add a <title> describing this page specifically, around 50 characters.",
        }
    }
    fn check(&self, page: &PageRecord, out: &mut Vec<Issue>) {
        // `None` only. A present-but-empty title is a different finding and
        // belongs to `title.too-short`; merging them would lose the
        // distinction between markup that forgot a title and markup that has
        // an empty one.
        if page.title.is_none() {
            out.push(Issue {
                rule_id: self.meta().id,
                severity: self.meta().severity,
                detail: None,
            });
        }
    }
}

pub struct TitleTooLong;

impl PageRule for TitleTooLong {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "title.too-long",
            severity: Severity::Warning,
            description: "The title is long enough that search results will truncate it.",
            remediation: "Trim it to about 60 characters, keeping the distinguishing words first.",
        }
    }
    fn check(&self, page: &PageRecord, out: &mut Vec<Issue>) {
        let Some(title) = page.title.as_deref() else {
            return;
        };
        let chars = title.chars().count();
        if chars > MAX_CHARS {
            out.push(Issue {
                rule_id: self.meta().id,
                severity: self.meta().severity,
                detail: Some(format!("{chars} characters")),
            });
        }
    }
}

pub struct TitleTooShort;

impl PageRule for TitleTooShort {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "title.too-short",
            severity: Severity::Warning,
            description: "The title is short enough that it is probably not describing the page.",
            remediation: "Expand it to around 50 characters. An empty title counts here.",
        }
    }
    fn check(&self, page: &PageRecord, out: &mut Vec<Issue>) {
        let Some(title) = page.title.as_deref() else {
            return;
        };
        let chars = title.chars().count();
        if chars < MIN_CHARS {
            out.push(Issue {
                rule_id: self.meta().id,
                severity: self.meta().severity,
                detail: Some(format!("{chars} characters")),
            });
        }
    }
}

pub struct MultipleTitles;

impl PageRule for MultipleTitles {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "title.multiple",
            severity: Severity::Warning,
            description: "The page contains more than one <title> element.",
            remediation: "Keep the first and remove the rest. Only the first is used, so the others are invisible edits.",
        }
    }
    fn check(&self, page: &PageRecord, out: &mut Vec<Issue>) {
        if page.title_count > 1 {
            out.push(Issue {
                rule_id: self.meta().id,
                severity: self.meta().severity,
                detail: Some(format!("{} <title> elements", page.title_count)),
            });
        }
    }
}

/// Titles shared by more than one page.
///
/// A `SiteRule`: a title is only duplicate relative to pages that may not have
/// been crawled when this one was.
pub struct DuplicateTitle;

impl SiteRule for DuplicateTitle {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "title.duplicate",
            severity: Severity::Warning,
            description: "More than one page uses this title.",
            remediation: "Give each page a title naming what only that page covers.",
        }
    }
    fn check(&self, store: &Store) -> Result<Vec<(String, Issue)>, StoreError> {
        // NULL is excluded on both sides. Two pages with no title are two
        // `title.missing` findings, not one shared title — and in SQL a NULL
        // never equals another NULL anyway, so this is explicit about what is
        // already true rather than relying on it.
        let mut stmt = store.conn().prepare(
            "SELECT url, title FROM pages \
             WHERE title IS NOT NULL AND title IN ( \
               SELECT title FROM pages WHERE title IS NOT NULL \
               GROUP BY title HAVING count(*) > 1) \
             ORDER BY url",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (url, title) = row?;
            out.push((
                url,
                Issue {
                    rule_id: self.meta().id,
                    severity: self.meta().severity,
                    detail: Some(title),
                },
            ));
        }
        Ok(out)
    }
}
