//! What a rule declares, and what it finds.

use serde::{Deserialize, Serialize};

/// How urgent a finding is.
///
/// Ordered most-urgent-first so a plain `sort()` puts critical at the top; the
/// grid sorts by this column and "critical after notice" is a wrong report.
///
/// There is deliberately no `Pass`. The spec's fourth state is a UI rendering
/// of "checked, nothing found", and storing a row per rule per page to record
/// absence would be 15M rows on a 500k crawl. A pass is the absence of an
/// issue, not a kind of issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Critical,
    Warning,
    Notice,
}

impl Severity {
    /// The form stored in SQL, emitted in JSON, and accepted by `--fail-on`.
    ///
    /// One spelling for all three on purpose: a column, an export and a CLI
    /// flag that disagree about capitalisation is a filter that silently
    /// matches nothing.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::Warning => "warning",
            Self::Notice => "notice",
        }
    }

    /// Parses the stored form. `None` for anything unrecognised.
    ///
    /// Never defaults. A corrupt file, or one written by a newer build with a
    /// severity this one has never heard of, must not quietly become a
    /// mis-severitied report — the caller decides what to do about it.
    ///
    /// Inherent rather than `FromStr` because the failure carries no detail
    /// worth an error type: the input either is one of three strings or is not.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "critical" => Some(Self::Critical),
            "warning" => Some(Self::Warning),
            "notice" => Some(Self::Notice),
            _ => None,
        }
    }
}

/// Everything a rule declares about itself.
///
/// `id` is stable forever: it appears in `--fail-on`, in exported reports, and
/// in saved `.pounce` files. Renaming one breaks a user's CI silently, since a
/// filter on a vanished id matches nothing rather than erroring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleMeta {
    pub id: &'static str,
    /// What was found, in the user's words.
    pub description: &'static str,
    /// What to do about it. A finding without a fix is noise.
    pub remediation: &'static str,
    pub severity: Severity,
}

/// One finding against one page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub rule_id: &'static str,
    /// Copied from the rule rather than looked up, because it is stored on the
    /// row: the grid filters millions of issues by severity, and a per-row join
    /// to rule metadata is exactly the query pattern M3 exists to avoid.
    pub severity: Severity,
    /// `None` when the rule id says everything. An empty string would render
    /// as a blank cell rather than as nothing.
    pub detail: Option<String>,
}
