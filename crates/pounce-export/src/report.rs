//! What the report says, separately from how it is typeset.
//!
//! Two formats carry the same argument — a PDF for sending and a Word file for
//! rebranding — and the fastest way to make them disagree is to gather the
//! facts twice. This module owns the facts; `pdf` and `docx` own the layout.

use crate::ExportError;
use pounce_store::{CrawlOverview, IssueOverview, SitemapSummary, Store};
use std::collections::BTreeMap;

/// Example URLs listed under each finding.
///
/// Enough to recognise the pattern, not so many that the report becomes the
/// spreadsheet. The count above them is the real number, and the line beneath
/// says how many were not listed.
pub const EXAMPLES: usize = 5;

/// What the caller has to tell the report, because the engine cannot know it.
pub struct ReportMeta<'a> {
    /// Today, already formatted. Written where the reader can see it, because
    /// an audit without a date is not evidence of anything — and formatted by
    /// the caller, because the engine has neither a clock nor a locale.
    pub date: &'a str,
    /// The file this came from, so a printed page can be traced back.
    pub file: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportSummary {
    pub findings: u64,
    pub pages: u64,
}

/// One finding, in the words the report uses.
pub struct Finding {
    /// "Issue", "Warning", "Opportunity" — the client-report vocabulary, not
    /// the registry's severity.
    pub label: &'static str,
    /// A hex colour for the label, and the class name in the PDF's stylesheet.
    pub tone: &'static str,
    pub hex: &'static str,
    pub sentence: String,
    pub remedy: String,
    pub urls: Vec<String>,
    pub affected: u64,
}

/// Everything the report draws from, gathered once.
pub struct Report {
    pub site: String,
    pub overview: CrawlOverview,
    pub issues: IssueOverview,
    pub maps: SitemapSummary,
    pub findings: Vec<Finding>,
}

impl Report {
    pub fn summary(&self) -> ReportSummary {
        ReportSummary {
            findings: self.findings.len() as u64,
            pages: self.overview.crawled,
        }
    }

    /// The five sentences under "Not checked in this version".
    ///
    /// Shared with the panel and the workbook by convention rather than by
    /// code, which is a seam worth watching: three places say this and they
    /// must not drift.
    pub fn not_checked() -> [&'static str; 5] {
        [
            "hreflang",
            "structured data",
            "pagination",
            "JavaScript rendering",
            "page speed",
        ]
    }
}

fn rank(severity: &str) -> u8 {
    match severity {
        "critical" => 0,
        "warning" => 1,
        _ => 2,
    }
}

/// The words a client report uses, from the severity the rule declared.
///
/// "Critical" is a word for engineers; "Issue", "Warning" and "Opportunity" is
/// how the conversation actually runs.
fn severity_words(severity: &str) -> (&'static str, &'static str, &'static str) {
    match severity {
        "critical" => ("Issue", "critical", "B42318"),
        "warning" => ("Warning", "warning", "8A5A00"),
        _ => ("Opportunity", "notice", "3B5BA5"),
    }
}

/// Reads the crawl into the shape a report needs.
///
/// `rules` maps a rule id to its sentence and its remedy. Passed in rather
/// than looked up: the registry lives in `pounce-audit`, and an exporter that
/// depended on it could not be used by anything that did not.
pub fn gather(
    store: &Store,
    rules: &BTreeMap<String, (String, String)>,
) -> Result<Report, ExportError> {
    let overview = store.crawl_overview()?;
    let issues = store.issue_overview()?;
    let maps = store.sitemap_summary()?;

    let site: String = store
        .conn()
        .query_row("SELECT seed_url FROM crawl WHERE id = 1", [], |r| r.get(0))
        .unwrap_or_else(|_| String::from("this crawl"));

    // Worst first, then by how much of the site it touches. `by_rule` arrives
    // ordered by count, which is the volume of a problem rather than its
    // seriousness — and a report that opens on an Opportunity affecting 24
    // images, above an Issue affecting 5 pages, is not the prioritised list it
    // claims to be.
    let mut by_rule = issues.by_rule.clone();
    by_rule.sort_by(|a, b| {
        rank(&a.severity)
            .cmp(&rank(&b.severity))
            .then(b.urls.cmp(&a.urls))
            .then(a.rule_id.cmp(&b.rule_id))
    });

    let mut findings = Vec::new();
    for row in &by_rule {
        let (label, tone, hex) = severity_words(&row.severity);
        let (sentence, remedy) = rules
            .get(&row.rule_id)
            .cloned()
            .unwrap_or_else(|| (row.rule_id.clone(), String::new()));

        let mut stmt = store
            .conn()
            .prepare("SELECT url FROM issues WHERE rule_id = ?1 ORDER BY url LIMIT ?2")?;
        let urls = stmt
            .query_map(rusqlite::params![row.rule_id, EXAMPLES as i64], |r| {
                r.get::<_, String>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;

        findings.push(Finding {
            label,
            tone,
            hex,
            sentence,
            remedy,
            urls,
            affected: row.urls,
        });
    }

    Ok(Report {
        site,
        overview,
        issues,
        maps,
        findings,
    })
}

/// `1,389`. Hand-rolled because a report full of `1389` reads as a part number,
/// and a locale-aware formatter is a dependency for one grouping character.
pub fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The sentence a JavaScript-built site needs at the top of its report.
///
/// `None` when the crawl shows no sign of one. It goes above the findings
/// rather than in a footnote because it changes how every number below it
/// should be read.
pub fn javascript_note(overview: &CrawlOverview) -> Option<String> {
    (overview.js_shell > 0).then(|| {
        format!(
            "{} of these pages arrived with almost no text in them. This site probably \
             builds its pages with JavaScript, which this version does not run — so the \
             findings below understate what a search engine would see.",
            thousands(overview.js_shell)
        )
    })
}

/// The sitemap paragraph, or `None` when there was no sitemap to compare with.
pub fn sitemap_note(maps: &SitemapSummary) -> Option<String> {
    (maps.files > 0).then(|| {
        format!(
            "The sitemap lists {} URLs. {} of them are reached by no link on the site, \
             and {} crawled pages are listed in no sitemap.",
            thousands(maps.urls),
            thousands(maps.not_crawled),
            thousands(maps.not_listed)
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thousands_groups_from_the_right() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_000), "1,000");
        assert_eq!(thousands(1_389), "1,389");
        assert_eq!(thousands(1_048_575), "1,048,575");
    }

    #[test]
    fn severity_becomes_the_word_a_client_report_uses() {
        assert_eq!(severity_words("critical").0, "Issue");
        assert_eq!(severity_words("warning").0, "Warning");
        assert_eq!(severity_words("notice").0, "Opportunity");
        // Worst first, and an unknown severity sorts last rather than first —
        // a rule with a severity this build does not know must not lead the
        // report.
        assert!(rank("critical") < rank("warning"));
        assert!(rank("warning") < rank("something-new"));
    }
}
