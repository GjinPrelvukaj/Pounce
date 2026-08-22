//! What a rule declares, and what it finds.

use serde::{Deserialize, Serialize};

/// How urgent a finding is.
///
/// **Read this before grading a new rule.** The levels are defined by what the
/// finding says happened, not by how large the fix is:
///
/// - **Critical** — assume the page is broken. A URL that fails to serve what
///   it promises (a 5xx, a redirect that never resolves), serves content a
///   browser refuses (mixed content), carries an instruction to search engines
///   that names something impossible (a canonical pointing at a non-200), or
///   declares no identity at all (`title.missing` — the one absence graded
///   Critical, because the title is the page's declared subject and nothing
///   else can supply it). Nobody chose this state; treat it as breakage.
/// - **Warning** — a real defect on a page that otherwise works and can be
///   indexed: duplicated or truncated signals, thin or missing supporting
///   content, links pointed at dead URLs, a working `noindex` nobody may have
///   meant to set. It ranks or converts worse than it should until fixed.
/// - **Notice** — nothing is wrong. The markup is legal (several `<h1>`), the
///   mechanism is being used correctly (a canonical pointing at its original),
///   or there is only headroom to gain (an oversized image, a short
///   description). Review invited; action optional.
///
/// Two tests a grade must survive before it ships:
///
/// 1. *Could the finding be the site working as someone intended?* Then it is
///    not Critical — a 404 is usually a decision someone made, which is why it
///    sits below a 5xx.
/// 2. *Is anything wrong at all?* If the answer is no, it is not a Warning,
///    however much improvement is possible.
///
/// When torn between two levels, tiebreak on the reader triaging a report:
/// what may they safely scroll past?
///
/// The shipped grades are pinned in `tests/severity_review.rs` alongside the
/// rulings these definitions encode; regrading a rule means editing both, on
/// purpose.
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
    /// Graded against the standard documented on [`Severity`] — read it before
    /// picking, and expect `tests/severity_review.rs` to hold the grade to it.
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
