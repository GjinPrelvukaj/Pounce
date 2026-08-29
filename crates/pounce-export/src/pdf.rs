//! The report as a PDF — the thing an agency sends a client.
//!
//! **Written as HTML and laid out by `printpdf`.** Hand-placing text on a page
//! means owning line breaking, and line breaking needs font metrics; the same
//! report as markup gets a real layout engine for the price of a stylesheet,
//! and a stylesheet is a thing a person can read and change.
//!
//! No font is embedded: with none supplied the layout falls back to the PDF
//! built-in Helvetica, which every reader has and which costs zero bytes.
//
// ponytail: built-in Helvetica, not the product's Inter. Embedding Inter means
// vendoring a ~300 kB TTF and carrying its OFL notice; worth doing when the
// report becomes something a client sees more often than once.

use crate::ExportError;
use crate::report::{
    Report, ReportMeta, ReportSummary, gather, javascript_note, sitemap_note, thousands,
};
use pounce_store::Store;
use printpdf::{GeneratePdfOptions, PdfDocument, PdfSaveOptions};
use std::collections::BTreeMap;
use std::path::Path;

/// XML-escapes text for the markup.
///
/// The renderer parses XML, so an unescaped `&` in a URL is a parse failure and
/// an unescaped `<` in a title silently eats the rest of the line. Only these
/// three: named entities beyond the five XML ones are *not* decoded by this
/// renderer, so everything else is written as the character itself.
fn esc(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub fn export_report(
    store: &Store,
    rules: &BTreeMap<String, (String, String)>,
    meta: &ReportMeta<'_>,
    path: &Path,
) -> Result<ReportSummary, ExportError> {
    let report = gather(store, rules)?;
    let html = render(&report, meta);

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
    std::fs::write(
        path,
        document.save(&PdfSaveOptions::default(), &mut warnings),
    )?;

    Ok(report.summary())
}

/// The stylesheet, and the report's whole visual argument.
///
/// One accent, on the wordmark alone; everything else is ink and grey, with
/// severity carried by a word before it is carried by a colour — the same rule
/// the interface follows, so a reader who cannot separate the hues still reads
/// "Issue".
const STYLE: &str = "
body { font-family: sans-serif; font-size: 10px; color: #23232B; line-height: 1.45; }
.brand { font-size: 9px; color: #5A3FD6; letter-spacing: 1px; }
h1 { font-size: 24px; color: #14141A; margin-top: 4px; margin-bottom: 2px; }
.meta { font-size: 10px; color: #6B6B76; margin-top: 0px; }
.rule { border-bottom: 1px solid #DCDCE3; margin-top: 14px; margin-bottom: 14px; }
h2 { font-size: 13px; color: #14141A; margin-bottom: 6px; }
.stats { display: flex; }
.stat { width: 96px; }
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

fn render(report: &Report, meta: &ReportMeta<'_>) -> String {
    let mut out = String::with_capacity(8_192);
    out.push_str("<html><head><style>");
    out.push_str(STYLE);
    out.push_str("</style></head><body>");

    out.push_str("<div class=\"brand\">POUNCE</div>");
    out.push_str("<h1>Search audit</h1>");
    out.push_str(&format!(
        "<p class=\"meta\">{} · {}</p>",
        esc(&report.site),
        esc(meta.date)
    ));
    out.push_str("<div class=\"rule\"></div>");

    // The four numbers that describe the crawl, before anything is judged.
    out.push_str("<div class=\"stats\">");
    for (n, label) in [
        (report.overview.crawled, "pages crawled"),
        (report.issues.pages_with_issues, "with something to fix"),
        (report.issues.total_issues, "findings"),
        (report.overview.noindex, "not indexable"),
    ] {
        out.push_str(&format!(
            "<div class=\"stat\"><div class=\"n\">{}</div><div class=\"l\">{label}</div></div>",
            thousands(n)
        ));
    }
    out.push_str("</div>");

    if let Some(note) = javascript_note(&report.overview) {
        out.push_str(&format!("<p class=\"note\">{}</p>", esc(&note)));
    }

    out.push_str("<div class=\"rule\"></div>");
    out.push_str("<h2>What to fix</h2>");
    if report.findings.is_empty() {
        out.push_str(
            "<p class=\"note\">Nothing. Every check this version runs passed on every page.</p>",
        );
    }
    for finding in &report.findings {
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

    if let Some(note) = sitemap_note(&report.maps) {
        out.push_str("<div class=\"rule\"></div>");
        out.push_str("<h2>The sitemap against the crawl</h2>");
        out.push_str(&format!("<p class=\"note\">{}</p>", esc(&note)));
    }

    out.push_str("<div class=\"rule\"></div>");
    out.push_str("<h2>Not checked in this version</h2>");
    out.push_str(&format!(
        "<p class=\"note\">Everything above ran on every page. These did not run at all, \
         so this report says nothing about them either way: {}.</p>",
        Report::not_checked().join(", ")
    ));

    out.push_str(&format!(
        "<p class=\"foot\">Pounce · {} · {}</p>",
        esc(meta.file),
        esc(meta.date)
    ));
    out.push_str("</body></html>");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_characters_in_data_are_escaped() {
        // A title containing `<` would otherwise eat the rest of the report,
        // and a `&` in a query string is a parse error rather than a character.
        assert_eq!(esc("a & b"), "a &amp; b");
        assert_eq!(esc("<script>"), "&lt;script&gt;");
        assert_eq!(esc("?a=1&b=2"), "?a=1&amp;b=2");
    }
}
