//! The shapes a rule can take.

use crate::issue::{Issue, RuleMeta};
use pounce_parse::PageRecord;

/// A check that needs nothing but the page in front of it.
///
/// Deliberately handed a `&PageRecord` and nothing else. It **cannot** issue a
/// query, which is what keeps Gate M2's 10% wall-time budget enforceable by the
/// compiler rather than by convention — after the scaling fix that budget is
/// ~14 seconds at 500k, and at rule 23 of 30 a convention would have lost.
pub trait PageRule: Send + Sync {
    fn meta(&self) -> RuleMeta;
    /// Push one `Issue` per finding. Runs on the crawl's hot path: no I/O, and
    /// no allocation beyond the issues themselves.
    fn check(&self, page: &PageRecord, out: &mut Vec<Issue>);
}
