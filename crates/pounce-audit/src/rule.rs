//! The shapes a rule can take.

use crate::issue::{Issue, RuleMeta};
use pounce_parse::PageRecord;
use pounce_store::{Store, StoreError};

/// A check that needs nothing but the page in front of it.
///
/// Deliberately handed a `&PageRecord` and nothing else. It **cannot** issue a
/// query, which is what keeps Gate M2's 10% wall-time budget enforceable by the
/// compiler rather than by convention — after the scaling fix that budget is
/// ~14 seconds at 500k, and at rule 23 of 30 a convention would have lost.
pub trait PageRule: Send + Sync {
    fn meta(&self) -> RuleMeta;

    /// Whether this rule has anything to say about this body at all.
    ///
    /// **Defaults to HTML only**, because most rules are about markup and a
    /// non-HTML body has none. A `sitemap.xml` has no `<title>`, and reporting
    /// one as missing is a false positive on a file that is working correctly
    /// — the same holds for a PDF with no `<h1>` and an image with no meta
    /// description.
    ///
    /// The default is the safe one and the exceptions opt out: response-level
    /// rules apply to *any* resource, since a 404 PDF is still a 404.
    fn applies(&self, page: &PageRecord) -> bool {
        page.kind == pounce_parse::BodyKind::Html
    }
    /// Push one `Issue` per finding. Runs on the crawl's hot path: no I/O, and
    /// no allocation beyond the issues themselves.
    fn check(&self, page: &PageRecord, out: &mut Vec<Issue>);
}

/// A check that needs the whole crawl.
///
/// Runs once, after the last page lands, against indexed columns. That is not
/// the "post-pass" T2.2 forbids: the prohibition is on a second pass over page
/// *bodies*, and these are `GROUP BY`/join queries that touch none and
/// allocate nothing proportional to crawl size.
///
/// Returns `(url, Issue)` pairs because a site rule *discovers* which pages are
/// affected, where a page rule is already looking at one.
pub trait SiteRule: Send + Sync {
    fn meta(&self) -> RuleMeta;
    fn check(&self, store: &Store) -> Result<Vec<(String, Issue)>, StoreError>;
}
