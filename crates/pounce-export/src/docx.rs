//! The report as a Word document — the same argument, editable.
//!
//! The reason this format exists is that the first thing an agency does with a
//! client report is put their own name on it. That makes the requirement
//! specific: **real Word styles, not hand-set formatting.** A heading that is
//! "18pt bold dark grey" is eleven separate decisions to undo; a heading that
//! is `Heading 1` changes for the whole document when someone picks a new
//! theme, which is what they are going to do.
//!
//! So every paragraph carries a style id, the styles are defined once at the
//! top, and direct formatting is used only where it carries meaning a style
//! cannot — the colour of a severity word, which is content rather than theme.

use crate::ExportError;
use crate::report::{
    Report, ReportMeta, ReportSummary, gather, javascript_note, sitemap_note, thousands,
};
use docx_rs::{AlignmentType, Docx, Paragraph, Run, RunFonts, Style, StyleType};
use pounce_store::Store;
use std::collections::BTreeMap;
use std::path::Path;

/// Word sizes runs in half-points, so every size here is doubled.
const fn half_points(pt: usize) -> usize {
    pt * 2
}

/// The document's styles, defined once.
///
/// `Heading1` and `Heading2` are Word's own built-in ids: defining them here
/// sets what they look like in this file, and leaves them recognisable to
/// Word's style gallery, the navigation pane, and any theme applied later.
fn styles(docx: Docx) -> Docx {
    docx.add_style(
        Style::new("Title", StyleType::Paragraph)
            .name("Title")
            .size(half_points(24))
            .color("14141A")
            .bold(),
    )
    .add_style(
        Style::new("Heading1", StyleType::Paragraph)
            .name("heading 1")
            .size(half_points(14))
            .color("14141A")
            .bold(),
    )
    .add_style(
        Style::new("Heading2", StyleType::Paragraph)
            .name("heading 2")
            .size(half_points(11))
            .color("14141A")
            .bold(),
    )
    .add_style(
        Style::new("Quiet", StyleType::Paragraph)
            .name("Quiet")
            .size(half_points(9))
            .color("6B6B76"),
    )
    .add_style(
        Style::new("Evidence", StyleType::Paragraph)
            .name("Evidence")
            .size(half_points(8))
            .color("6B6B76"),
    )
}

fn quiet(text: &str) -> Paragraph {
    Paragraph::new()
        .style("Quiet")
        .add_run(Run::new().add_text(text))
}

/// A URL, in the monospace face URLs are read in.
///
/// The one place this document uses a face rather than a style: an address is
/// read character by character, and a proportional font makes `rn` and `m` the
/// same shape.
fn evidence(text: &str) -> Paragraph {
    Paragraph::new().style("Evidence").add_run(
        Run::new()
            .add_text(text)
            .fonts(RunFonts::new().ascii("Consolas").hi_ansi("Consolas")),
    )
}

pub fn export_docx(
    store: &Store,
    rules: &BTreeMap<String, (String, String)>,
    meta: &ReportMeta<'_>,
    path: &Path,
) -> Result<ReportSummary, ExportError> {
    let report = gather(store, rules)?;
    let mut docx = styles(Docx::new());

    docx = docx
        .add_paragraph(
            Paragraph::new().style("Quiet").add_run(
                Run::new()
                    .add_text("POUNCE")
                    .color("5A3FD6")
                    .size(half_points(8)),
            ),
        )
        .add_paragraph(
            Paragraph::new()
                .style("Title")
                .add_run(Run::new().add_text("Search audit")),
        )
        .add_paragraph(quiet(&format!("{} · {}", report.site, meta.date)));

    // The four numbers, as one line each rather than a table: a table is a
    // thing someone has to fight to restyle, and this is four facts.
    docx = docx.add_paragraph(
        Paragraph::new()
            .style("Heading2")
            .add_run(Run::new().add_text("The crawl")),
    );
    for (n, label) in [
        (report.overview.crawled, "pages crawled"),
        (report.issues.pages_with_issues, "with something to fix"),
        (report.issues.total_issues, "findings"),
        (report.overview.noindex, "not indexable"),
    ] {
        docx = docx.add_paragraph(
            Paragraph::new().add_run(
                Run::new()
                    .add_text(format!("{}  {label}", thousands(n)))
                    .size(half_points(10)),
            ),
        );
    }

    if let Some(note) = javascript_note(&report.overview) {
        docx = docx.add_paragraph(quiet(&note));
    }

    docx = docx.add_paragraph(
        Paragraph::new()
            .style("Heading1")
            .add_run(Run::new().add_text("What to fix")),
    );
    if report.findings.is_empty() {
        docx = docx.add_paragraph(quiet(
            "Nothing. Every check this version runs passed on every page.",
        ));
    }
    for finding in &report.findings {
        // Severity is direct formatting on purpose: the colour is part of what
        // the word means, not part of the document's theme, and it should
        // survive a restyle rather than be swept up by one.
        docx = docx.add_paragraph(
            Paragraph::new()
                .add_run(
                    Run::new()
                        .add_text(finding.label)
                        .color(finding.hex)
                        .size(half_points(9)),
                )
                .add_run(
                    Run::new()
                        .add_text(format!("    {} URLs", thousands(finding.affected)))
                        .color("6B6B76")
                        .size(half_points(9)),
                ),
        );
        docx = docx.add_paragraph(
            Paragraph::new().add_run(
                Run::new()
                    .add_text(&finding.sentence)
                    .size(half_points(11))
                    .color("14141A"),
            ),
        );
        if !finding.remedy.is_empty() {
            docx = docx.add_paragraph(quiet(&finding.remedy));
        }
        for url in &finding.urls {
            docx = docx.add_paragraph(evidence(url));
        }
        let listed = finding.urls.len() as u64;
        if finding.affected > listed {
            docx = docx.add_paragraph(
                Paragraph::new().style("Evidence").add_run(
                    Run::new()
                        .add_text(format!("and {} more", thousands(finding.affected - listed)))
                        .color("9A9AA4"),
                ),
            );
        }
        docx = docx.add_paragraph(Paragraph::new());
    }

    if let Some(note) = sitemap_note(&report.maps) {
        docx = docx
            .add_paragraph(
                Paragraph::new()
                    .style("Heading1")
                    .add_run(Run::new().add_text("The sitemap against the crawl")),
            )
            .add_paragraph(quiet(&note));
    }

    docx = docx
        .add_paragraph(
            Paragraph::new()
                .style("Heading1")
                .add_run(Run::new().add_text("Not checked in this version")),
        )
        .add_paragraph(quiet(&format!(
            "Everything above ran on every page. These did not run at all, so this report \
             says nothing about them either way: {}.",
            Report::not_checked().join(", ")
        )))
        .add_paragraph(
            Paragraph::new()
                .style("Evidence")
                .align(AlignmentType::Right)
                .add_run(Run::new().add_text(format!("Pounce · {} · {}", meta.file, meta.date))),
        );

    let file = std::fs::File::create(path)?;
    docx.build()
        .pack(file)
        .map_err(|e| ExportError::Io(std::io::Error::other(e.to_string())))?;
    Ok(report.summary())
}
