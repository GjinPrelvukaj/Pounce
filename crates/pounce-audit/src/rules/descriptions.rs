//! Meta description findings.
//!
//! Lengths count characters rather than bytes, for the reason `titles.rs`
//! gives: a byte count misreports accented and non-Latin text, and does so only
//! for the sites least likely to notice.

use crate::issue::{Issue, RuleMeta, Severity};
use crate::registry::{Registry, RegistryError};
use crate::rule::{PageRule, SiteRule};
use pounce_parse::PageRecord;
use pounce_store::{Store, StoreError};

/// Above this, search results truncate the description.
const MAX_CHARS: usize = 160;
/// Below this, the description is leaving most of its space unused.
const MIN_CHARS: usize = 70;

/// Entity names whose truncated remains show up at the end of a description
/// that was cut to a character limit by a CMS.
///
/// A list rather than "any `&` followed by letters", because `Q&A` and
/// `Ben & Jerry` end that way and are not truncated. Matching a known prefix is
/// the difference between a finding and a nuisance.
const ENTITY_NAMES: &[&str] = &[
    "amp", "apos", "hellip", "ldquo", "lsquo", "mdash", "nbsp", "ndash", "quot", "rdquo", "rsquo",
    "lt", "gt",
];

pub fn register(registry: &mut Registry) -> Result<(), RegistryError> {
    registry.register_page(Box::new(MissingDescription))?;
    registry.register_page(Box::new(DescriptionTooLong))?;
    registry.register_page(Box::new(DescriptionTooShort))?;
    registry.register_page(Box::new(TruncatedEntity))?;
    registry.register_site(Box::new(DuplicateDescription))?;
    Ok(())
}

pub struct MissingDescription;

impl PageRule for MissingDescription {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "description.missing",
            severity: Severity::Warning,
            description: "The page has no meta description.",
            remediation: "Add one of roughly 150 characters describing what this page offers.",
        }
    }
    fn check(&self, page: &PageRecord, out: &mut Vec<Issue>) {
        // `None` only — a present-but-empty description is `too-short`, and
        // merging them would lose which mistake the markup made.
        if page.meta_description.is_none() {
            out.push(Issue {
                rule_id: self.meta().id,
                severity: self.meta().severity,
                detail: None,
            });
        }
    }
}

pub struct DescriptionTooLong;

impl PageRule for DescriptionTooLong {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "description.too-long",
            severity: Severity::Warning,
            description: "The description is long enough that search results will cut it off.",
            remediation: "Trim to about 155 characters, putting the reason to click first.",
        }
    }
    fn check(&self, page: &PageRecord, out: &mut Vec<Issue>) {
        let Some(text) = page.meta_description.as_deref() else {
            return;
        };
        let chars = text.chars().count();
        if chars > MAX_CHARS {
            out.push(Issue {
                rule_id: self.meta().id,
                severity: self.meta().severity,
                detail: Some(format!("{chars} characters")),
            });
        }
    }
}

pub struct DescriptionTooShort;

impl PageRule for DescriptionTooShort {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "description.too-short",
            severity: Severity::Notice,
            // Notice, not Warning: a short description still works, it just
            // wastes space. A missing one is worse and is already a Warning.
            description: "The description is short enough that it is leaving space unused.",
            remediation: "Expand toward 150 characters. Half a line of result text is half the reason to click.",
        }
    }
    fn check(&self, page: &PageRecord, out: &mut Vec<Issue>) {
        let Some(text) = page.meta_description.as_deref() else {
            return;
        };
        let chars = text.chars().count();
        if chars < MIN_CHARS {
            out.push(Issue {
                rule_id: self.meta().id,
                severity: self.meta().severity,
                detail: Some(format!("{chars} characters")),
            });
        }
    }
}

pub struct TruncatedEntity;

impl PageRule for TruncatedEntity {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "description.truncated-entity",
            severity: Severity::Warning,
            description: "The description ends in a half-written HTML entity, so a CMS cut it mid-character.",
            remediation: "Truncate on a character boundary rather than a byte or entity one, or shorten the source text.",
        }
    }
    fn check(&self, page: &PageRecord, out: &mut Vec<Issue>) {
        let Some(text) = page.meta_description.as_deref() else {
            return;
        };
        if let Some(fragment) = dangling_entity(text) {
            out.push(Issue {
                rule_id: self.meta().id,
                severity: self.meta().severity,
                detail: Some(fragment),
            });
        }
    }
}

/// The half-written entity at the end of `text`, if there is one.
///
/// Runs on the **decoded** description, so a complete `&amp;` has already
/// become `&` and cannot be mistaken for a fragment. What survives decoding is
/// exactly the broken case: an ampersand, a partial name, and no semicolon.
fn dangling_entity(text: &str) -> Option<String> {
    let start = text.rfind('&')?;
    let tail = &text[start + 1..];
    if tail.is_empty() || tail.contains(';') {
        return None;
    }
    // No whitespace guard: it was tried and a mutation test showed it changed
    // no outcome. `Fish & Chips` leaves a tail of " Chips…", which matches no
    // entity name and no numeric form, so the checks below already reject it.
    // Numeric references: `&#82` or `&#x1F6`.
    let numeric = tail
        .strip_prefix("#x")
        .or_else(|| tail.strip_prefix("#X"))
        .map(|d| !d.is_empty() && d.chars().all(|c| c.is_ascii_hexdigit()))
        .or_else(|| {
            tail.strip_prefix('#')
                .map(|d| !d.is_empty() && d.chars().all(|c| c.is_ascii_digit()))
        })
        .unwrap_or(false);
    // A *complete* entity was decoded before we ever saw the text, so anything
    // still spelled out here is broken — including the full name, which is
    // truncated precisely because its semicolon was cut off.
    let named = ENTITY_NAMES.iter().any(|name| name.starts_with(tail));
    (numeric || named).then(|| format!("&{tail}"))
}

/// Descriptions shared by more than one page.
pub struct DuplicateDescription;

impl SiteRule for DuplicateDescription {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "description.duplicate",
            severity: Severity::Warning,
            description: "More than one page uses this meta description.",
            remediation: "Write one per page. A shared description tells a searcher nothing about which result to pick.",
        }
    }
    fn check(&self, store: &Store) -> Result<Vec<(String, Issue)>, StoreError> {
        // NULL excluded on both sides: two pages without a description are two
        // `description.missing` findings, not one shared description.
        let mut stmt = store.conn().prepare(
            "SELECT url, meta_description FROM pages \
             WHERE meta_description IS NOT NULL AND meta_description IN ( \
               SELECT meta_description FROM pages WHERE meta_description IS NOT NULL \
               GROUP BY meta_description HAVING count(*) > 1) \
             ORDER BY url",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (url, text) = row?;
            out.push((
                url,
                Issue {
                    rule_id: self.meta().id,
                    severity: self.meta().severity,
                    detail: Some(text),
                },
            ));
        }
        Ok(out)
    }
}
