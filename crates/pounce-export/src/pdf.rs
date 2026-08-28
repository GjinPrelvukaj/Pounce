//! The report an agency hands to a client.
//!
//! Not a table. The workbook is the data and this is the argument: what was
//! crawled, what is wrong with it in order of how much it matters, what to do
//! about each thing, and — the part most tools leave out — what was not
//! examined at all, so the document cannot be read as a clean bill for checks
//! that never ran.
//!
//! **Written as HTML and laid out by `printpdf`.** Hand-placing text on a page
//! means owning line breaking, and line breaking needs font metrics; the same
//! report as markup gets a real layout engine for the price of a stylesheet,
//! and the stylesheet is a thing a person can read and change. No font is
//! embedded: with none supplied the layout falls back to the PDF built-in
//! Helvetica, which every reader has and which costs zero bytes.
//
// ponytail: built-in Helvetica, not the product's Inter. Embedding Inter means
// vendoring a ~300 kB TTF and carrying its OFL notice; worth doing when the
// report becomes something a client sees more often than once.

use crate::ExportError;
use pounce_store::Store;
use printpdf::{GeneratePdfOptions, PdfDocument, PdfSaveOptions};
use std::collections::BTreeMap;
use std::path::Path;

/// Example URLs listed under each finding.
///
/// Enough to recognise the pattern, not so many that the report becomes the
/// spreadsheet. The count above them is the real number, and the line beneath
/// says how many were not listed.
const EXAMPLES: usize = 5;

/// What the caller has to tell the report, because the engine cannot know it.
pub struct ReportMeta<'a> {
    /// Today, already formatted. Written where the reader can see it, because
    /// an audit without a date is not evidence of anything.
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

/// XML-escapes text for the markup.
///
/// The renderer parses XML, so an unescaped `&` in a URL is a parse failure and
/// an unescaped `<` in a title silently eats the rest of the line. Only the
/// three that matter: the five XML entities are what the parser decodes, and
/// anything else — a `·`, an em dash — is written as the character itself.
fn esc(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// One finding, as the report talks about it.
struct Finding {
    label: &'static str,
    tone: &'static str,
    sentence: String,
    remedy: String,
    urls: Vec<String>,
    affected: u64,
}

/// The words a client report uses, from the severity the rule declared.
///
/// The same three the Issues panel uses. "Critical" is a word for engineers;
/// "Issue", "Warning" and "Opportunity" is how the conversation actually runs.
fn rank(severity: &str) -> u8 {
    match severity {
        "critical" => 0,
        "warning" => 1,
        _ => 2,
    }
}

fn severity_words(severity: &str) -> (&'static str, &'static str) {
    match severity {
        "critical" => ("Issue", "critical"),
        "warning" => ("Warning", "warning"),
        _ => ("Opportunity", "notice"),
    }
}

pub fn export_report(
    store: &Store,
    rules: &BTreeMap<String, (String, String)>,
    meta: &ReportMeta<'_>,
    path: &Path,
) -> Result<ReportSummary, ExportError> {
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
        let (label, tone) = severity_words(&row.severity);
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
            sentence,
            remedy,
            urls,
            affected: row.urls,
        });
    }

    let html = render(&site, meta, &overview, &issues, &maps, &findings);

    let mut warnings = Vec::new();
    let options = GeneratePdfOptions {
        margin_top: Some(18.0),
        margin_right: Some(18.0),
        margin_bottom: Some(18.0),
        margin_left: Some(18.0),
        show_page_numbers: Some(true),
        ..GeneratePdfOptions::default()
    };
    let document = PdfDocument::from_html(
        &html,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &options,
        &mut warnings,
    )
    .map_err(|e| ExportError::Io(std::io::Error::other(e)))?;
    let bytes = document.save(&PdfSaveOptions::default(), &mut warnings);
    std::fs::write(path, bytes)?;

    Ok(ReportSummary {
        findings: findings.len() as u64,
        pages: overview.crawled,
    })
}

/// The stylesheet, and the report's whole visual argument.
///
/// One accent, used only for the rule above each finding's severity; the rest
/// is set in ink and grey. Severity is a word before it is a colour, which is
/// the same rule the interface follows — a reader who cannot separate the hues
/// still reads "Issue".
const STYLE: &str = "
body { font-family: sans-serif; font-size: 10px; color: #23232B; line-height: 1.45; }
.brand { font-size: 9px; color: #5A3FD6; letter-spacing: 1px; }
h1 { font-size: 24px; color: #14141A; margin-top: 4px; margin-bottom: 2px; }
.meta { font-size: 10px; color: #6B6B76; margin-top: 0px; }
.rule { border-bottom: 1px solid #DCDCE3; margin-top: 14px; margin-bottom: 14px; }
h2 { font-size: 13px; color: #14141A; margin-bottom: 6px; }
.stats { display: flex; }
.stat { width: 88px; }
.stat .n { font-size: 19px; color: #14141A; }
.stat .l { font-size: 8px; color: #6B6B76; }
.finding { margin-bottom: 12px; }
.fh { display: flex; margin-bottom: 1px; }
.sev { font-size: 9px; width: 78px; }
.critical { color: #B42318; }
.warning { color: #8A5A00; }
.notice { color: #3B5BA5; }
.cnt { font-size: 9px; color: #6B6B76; }
.ft { font-size: 11px; color: #14141A; }
.fr { font-size: 9px; color: #4A4A55; }
.urls { font-size: 8px; color: #6B6B76; margin-top: 2px; }
.more { font-size: 8px; color: #9A9AA4; }
.note { font-size: 9px; color: #6B6B76; }
.foot { font-size: 8px; color: #9A9AA4; margin-top: 16px; }
";

fn render(
    site: &str,
    meta: &ReportMeta<'_>,
    overview: &pounce_store::CrawlOverview,
    issues: &pounce_store::IssueOverview,
    maps: &pounce_store::SitemapSummary,
    findings: &[Finding],
) -> String {
    let mut out = String::with_capacity(8_192);
    out.push_str("<html><head><style>");
    out.push_str(STYLE);
    out.push_str("</style></head><body>");

    out.push_str("<div class=\"brand\">POUNCE</div>");
    out.push_str("<h1>Search audit</h1>");
    out.push_str(&format!(
        "<p class=\"meta\">{} · {}</p>",
        esc(site),
        esc(meta.date)
    ));
    out.push_str("<div class=\"rule\"></div>");

    // The four numbers that describe the crawl, before anything is judged.
    out.push_str("<div class=\"stats\">");
    for (n, label) in [
        (overview.crawled, "pages crawled"),
        (issues.pages_with_issues, "with something to fix"),
        (issues.total_issues, "findings"),
        (overview.noindex, "not indexable"),
    ] {
        out.push_str(&format!(
            "<div class=\"stat\"><div class=\"n\">{}</div><div class=\"l\">{label}</div></div>",
            thousands(n)
        ));
    }
    out.push_str("</div>");

    // A site built with JavaScript is said so here, at the top, because it
    // changes how every number below it should be read.
    if overview.js_shell > 0 {
        out.push_str(&format!(
            "<p class=\"note\">{} of these pages arrived with almost no text in them. \
             This site probably builds its pages with JavaScript, which this version does \
             not run — so the findings below understate what a search engine would see.</p>",
            thousands(overview.js_shell)
        ));
    }

    out.push_str("<div class=\"rule\"></div>");
    out.push_str("<h2>What to fix</h2>");
    if findings.is_empty() {
        out.push_str(
            "<p class=\"note\">Nothing. Every check this version runs passed on every page.</p>",
        );
    }
    for finding in findings {
        out.push_str("<div class=\"finding\">");
        out.push_str(&format!(
            "<div class=\"fh\"><span class=\"sev {}\">{}</span>\
             <span class=\"cnt\">{} URLs</span></div>",
            finding.tone,
            finding.label,
            thousands(finding.affected)
        ));
        out.push_str(&format!(
            "<div class=\"ft\">{}</div>",
            esc(&finding.sentence)
        ));
        if !finding.remedy.is_empty() {
            out.push_str(&format!("<div class=\"fr\">{}</div>", esc(&finding.remedy)));
        }
        for url in &finding.urls {
            out.push_str(&format!("<div class=\"urls\">{}</div>", esc(url)));
        }
        let listed = finding.urls.len() as u64;
        if finding.affected > listed {
            out.push_str(&format!(
                "<div class=\"more\">and {} more</div>",
                thousands(finding.affected - listed)
            ));
        }
        out.push_str("</div>");
    }

    // The sitemap comparison, only when there was a sitemap to compare with.
    if maps.files > 0 {
        out.push_str("<div class=\"rule\"></div>");
        out.push_str("<h2>The sitemap against the crawl</h2>");
        out.push_str(&format!(
            "<p class=\"note\">The sitemap lists {} URLs. {} of them are reached by no link \
             on the site, and {} crawled pages are listed in no sitemap.</p>",
            thousands(maps.urls),
            thousands(maps.not_crawled),
            thousands(maps.not_listed)
        ));
    }

    out.push_str("<div class=\"rule\"></div>");
    out.push_str("<h2>Not checked in this version</h2>");
    out.push_str(
        "<p class=\"note\">Everything above ran on every page. These did not run at all, \
         so this report says nothing about them either way: hreflang, structured data, \
         pagination, JavaScript rendering, page speed.</p>",
    );

    out.push_str(&format!(
        "<p class=\"foot\">Pounce · {} · {}</p>",
        esc(meta.file),
        esc(meta.date)
    ));
    out.push_str("</body></html>");
    out
}

/// `1,389`. Hand-rolled because a report full of `1389` reads as a part number,
/// and a locale-aware formatter is a dependency for one grouping character.
fn thousands(n: u64) -> String {
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
    fn markup_characters_in_data_are_escaped() {
        // A title containing `<` would otherwise eat the rest of the report,
        // and a `&` in a query string is a parse error rather than a character.
        assert_eq!(esc("a & b"), "a &amp; b");
        assert_eq!(esc("<script>"), "&lt;script&gt;");
        assert_eq!(esc("?a=1&b=2"), "?a=1&amp;b=2");
    }
}
