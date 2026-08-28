//! The workbook an agency actually opens.
//!
//! CSV is a transfer format; a workbook is a deliverable. The difference is
//! not decoration — a header that stays put while you scroll, filters on every
//! column, and numbers that are numbers rather than text are what let someone
//! sort by word count or pivot by status without cleaning the file first.
//!
//! **One sheet per question, not one sheet per view.** Seven sheets of the same
//! pages with different columns is the same data seven times, which on a large
//! crawl is a file nobody can open. The sheets here are the things that are
//! genuinely different: the rows you were looking at, the findings, the
//! images, the sitemap comparison, and a summary that says what the crawl was.
//!
//! Written in `constant_memory` mode, one row alive at a time. The invariant
//! that keeps the dataset out of the UI keeps it out of the exporter too: a
//! workbook built in memory would be the one place in this product where a
//! million rows are held at once.

use crate::ExportError;
use pounce_store::{FilterSpec, SortSpec, Store};
use rust_xlsxwriter::{Color, Format, FormatAlign, Workbook, Worksheet};
use std::collections::BTreeMap;
use std::path::Path;

/// Excel's own ceiling, minus the header row.
///
/// A crawl can be larger than a spreadsheet. When it is, the workbook says so
/// rather than ending mid-list and looking complete — the same rule the folder
/// tree follows when a directory has more children than it will list.
pub const MAX_ROWS: u64 = 1_048_575;

/// What the workbook ended up holding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbookSummary {
    pub pages: u64,
    pub issues: u64,
    pub images: u64,
    pub sitemap: u64,
    /// Rows the filter matched that Excel could not hold.
    pub pages_not_written: u64,
}

/// The brand's violet, dark enough for white text on it.
fn header_format() -> Format {
    Format::new()
        .set_bold()
        .set_font_color(Color::White)
        .set_background_color(Color::RGB(0x5A_3F_D6))
        .set_align(FormatAlign::Left)
}

fn title_format() -> Format {
    Format::new().set_bold().set_font_size(14)
}

fn label_format() -> Format {
    Format::new().set_font_color(Color::RGB(0x6B_6B_76))
}

/// Writes the header row, freezes it, and turns on the filter dropdowns.
///
/// All three together, because they are one idea: a table you can work in. A
/// header that scrolls away is the first thing that makes a large export feel
/// like a dump.
fn head(sheet: &mut Worksheet, columns: &[(&str, f64)]) -> Result<(), ExportError> {
    let format = header_format();
    for (i, (name, width)) in columns.iter().enumerate() {
        let col = i as u16;
        sheet.write_string_with_format(0, col, *name, &format)?;
        sheet.set_column_width(col, *width)?;
    }
    sheet.set_freeze_panes(1, 0)?;
    sheet.autofilter(0, 0, 0, (columns.len() - 1) as u16)?;
    Ok(())
}

/// A nullable string cell.
///
/// `None` leaves the cell genuinely empty; `Some("")` writes an empty string.
/// Excel tells those apart — `ISBLANK` is true for one and false for the other
/// — which is exactly the distinction the store has carried since the parser,
/// and the one a "missing description" report turns on.
fn write_opt(
    sheet: &mut Worksheet,
    row: u32,
    col: u16,
    value: Option<String>,
) -> Result<(), ExportError> {
    match value {
        Some(text) => sheet.write_string(row, col, text)?,
        None => sheet.write_blank(row, col, &Format::default())?,
    };
    Ok(())
}

/// Writes the whole workbook to `path`.
///
/// `rules` maps a rule id to the sentence the interface shows for it, passed in
/// rather than looked up: the registry lives in `pounce-audit` and an exporter
/// that depended on it could not be used by anything that did not.
pub fn export_workbook(
    store: &Store,
    filters: &FilterSpec,
    sort: &SortSpec,
    rules: &BTreeMap<String, String>,
    path: &Path,
) -> Result<WorkbookSummary, ExportError> {
    let mut workbook = Workbook::new();
    let mut summary = WorkbookSummary::default();

    // Summary first, so the file opens on what the crawl was rather than on
    // row 1 of 400,000.
    write_summary(&mut workbook, store)?;
    summary.issues = write_issues(&mut workbook, store, rules)?;
    let (pages, dropped) = write_pages(&mut workbook, store, filters, sort)?;
    summary.pages = pages;
    summary.pages_not_written = dropped;
    summary.images = write_images(&mut workbook, store)?;
    summary.sitemap = write_sitemap(&mut workbook, store)?;

    workbook.save(path)?;
    Ok(summary)
}

fn write_summary(workbook: &mut Workbook, store: &Store) -> Result<(), ExportError> {
    let overview = store.crawl_overview()?;
    let issues = store.issue_overview()?;
    let maps = store.sitemap_summary()?;

    let sheet = workbook.add_worksheet();
    sheet.set_name("Summary")?;
    sheet.set_column_width(0, 46.0)?;
    sheet.set_column_width(1, 16.0)?;

    let title = title_format();
    let label = label_format();
    sheet.write_string_with_format(0, 0, "Pounce crawl summary", &title)?;

    // A free function rather than a closure: a closure capturing `row` holds it
    // borrowed for the rest of the block, and this sheet writes a few plain
    // lines outside the counted ones.
    fn line(
        sheet: &mut Worksheet,
        row: &mut u32,
        label: &Format,
        name: &str,
        value: i64,
    ) -> Result<(), ExportError> {
        sheet.write_string_with_format(*row, 0, name, label)?;
        sheet.write_number(*row, 1, value as f64)?;
        *row += 1;
        Ok(())
    }

    let mut row = 2u32;
    line(
        sheet,
        &mut row,
        &label,
        "Pages crawled",
        overview.crawled as i64,
    )?;
    line(
        sheet,
        &mut row,
        &label,
        "Still to fetch",
        overview.queued as i64,
    )?;
    line(
        sheet,
        &mut row,
        &label,
        "Never answered",
        overview.failed as i64,
    )?;
    line(
        sheet,
        &mut row,
        &label,
        "Indexable",
        overview.indexable as i64,
    )?;
    line(
        sheet,
        &mut row,
        &label,
        "Not indexable",
        overview.noindex as i64,
    )?;
    line(
        sheet,
        &mut row,
        &label,
        "Pages with something to fix",
        issues.pages_with_issues as i64,
    )?;
    line(
        sheet,
        &mut row,
        &label,
        "Findings in total",
        issues.total_issues as i64,
    )?;
    // The comparison only exists if there is something to compare against.
    // With no sitemap read, "18 pages missing from the sitemap" is true of the
    // arithmetic and false of the site — every page is missing from a file
    // that was never found, and reporting it as a finding invents one.
    if maps.files == 0 {
        sheet.write_string_with_format(
            row,
            0,
            "No sitemap was found to compare against",
            &label,
        )?;
        row += 1;
    } else {
        line(
            sheet,
            &mut row,
            &label,
            "URLs in the sitemap",
            maps.urls as i64,
        )?;
        line(
            sheet,
            &mut row,
            &label,
            "In the sitemap, not reached by any link",
            maps.not_crawled as i64,
        )?;
        line(
            sheet,
            &mut row,
            &label,
            "Crawled and indexable, missing from the sitemap",
            maps.not_listed as i64,
        )?;
    }
    if overview.js_shell > 0 {
        line(
            sheet,
            &mut row,
            &label,
            "Pages that arrived with almost no text (JavaScript?)",
            overview.js_shell as i64,
        )?;
    }

    // The same disclosure the panel carries. A spreadsheet that lists findings
    // and says nothing about coverage can be read as a clean bill for checks
    // that never ran, and this file outlives the window it came from.
    row += 1;
    sheet.write_string_with_format(row, 0, "Not checked in this version", &title_format())?;
    row += 1;
    for name in [
        "hreflang",
        "Structured data",
        "Pagination",
        "JavaScript rendering",
        "Page speed",
    ] {
        sheet.write_string_with_format(row, 0, name, &label)?;
        row += 1;
    }
    Ok(())
}

fn write_issues(
    workbook: &mut Workbook,
    store: &Store,
    rules: &BTreeMap<String, String>,
) -> Result<u64, ExportError> {
    let sheet = workbook.add_worksheet_with_constant_memory();
    sheet.set_name("Issues")?;
    head(
        sheet,
        &[
            ("Severity", 12.0),
            ("What is wrong", 62.0),
            ("URL", 70.0),
            ("Detail", 40.0),
            ("Rule", 26.0),
        ],
    )?;

    let critical = Format::new()
        .set_font_color(Color::RGB(0xB4_23_18))
        .set_bold();
    let warning = Format::new().set_font_color(Color::RGB(0x8A_5A_00));

    let mut stmt = store.conn().prepare(
        // Worst first, and by rule inside a severity, so the sheet opens on
        // the work rather than on whatever URL sorts first.
        "SELECT severity, rule_id, url, detail FROM issues \
         ORDER BY CASE severity WHEN 'critical' THEN 0 WHEN 'warning' THEN 1 ELSE 2 END, \
                  rule_id, url",
    )?;
    let mut rows = stmt.query([])?;
    let mut n = 0u64;
    while let Some(row) = rows.next()? {
        if n >= MAX_ROWS {
            break;
        }
        let at = (n + 1) as u32;
        let severity: String = row.get(0)?;
        let rule_id: String = row.get(1)?;
        let format = match severity.as_str() {
            "critical" => Some(&critical),
            "warning" => Some(&warning),
            _ => None,
        };
        match format {
            Some(f) => sheet.write_string_with_format(at, 0, &severity, f)?,
            None => sheet.write_string(at, 0, &severity)?,
        };
        sheet.write_string(
            at,
            1,
            rules.get(&rule_id).map(String::as_str).unwrap_or(&rule_id),
        )?;
        sheet.write_string(at, 2, row.get::<_, String>(2)?)?;
        write_opt(sheet, at, 3, row.get(3)?)?;
        sheet.write_string(at, 4, &rule_id)?;
        n += 1;
    }
    Ok(n)
}

fn write_pages(
    workbook: &mut Workbook,
    store: &Store,
    filters: &FilterSpec,
    sort: &SortSpec,
) -> Result<(u64, u64), ExportError> {
    let sheet = workbook.add_worksheet_with_constant_memory();
    sheet.set_name("Pages")?;
    head(
        sheet,
        &[
            ("URL", 70.0),
            ("Status", 9.0),
            ("Title", 50.0),
            ("Meta description", 60.0),
            ("H1", 40.0),
            ("Words", 9.0),
            ("Depth", 8.0),
            ("Bytes", 11.0),
            ("Type", 12.0),
            ("Indexable", 11.0),
            ("Canonical", 50.0),
            ("Response ms", 12.0),
        ],
    )?;

    let (where_sql, params) = filters.compile();
    let sql = format!(
        "SELECT p.url, p.status, p.title, p.meta_description, \
                json_extract(d.h1, '$[0]'), p.word_count, p.depth, p.size, p.kind, \
                p.noindex, p.canonical, p.elapsed_ms \
         FROM pages p LEFT JOIN page_detail d ON d.page_id = p.id {where_sql} {}",
        sort.compile()
    );
    let mut stmt = store.conn().prepare(&sql)?;
    let mut rows = stmt.query(rusqlite::params_from_iter(params.iter()))?;
    let mut n = 0u64;
    let mut dropped = 0u64;
    while let Some(row) = rows.next()? {
        if n >= MAX_ROWS {
            dropped += 1;
            continue;
        }
        let at = (n + 1) as u32;
        sheet.write_string(at, 0, row.get::<_, String>(0)?)?;
        sheet.write_number(at, 1, row.get::<_, i64>(1)? as f64)?;
        write_opt(sheet, at, 2, row.get(2)?)?;
        write_opt(sheet, at, 3, row.get(3)?)?;
        write_opt(sheet, at, 4, row.get(4)?)?;
        sheet.write_number(at, 5, row.get::<_, i64>(5)? as f64)?;
        sheet.write_number(at, 6, row.get::<_, i64>(6)? as f64)?;
        sheet.write_number(at, 7, row.get::<_, i64>(7)? as f64)?;
        sheet.write_string(at, 8, row.get::<_, String>(8)?)?;
        // A word, not a 0/1: this column is read by a person, and "TRUE" in a
        // column called Indexable means the opposite of what it says.
        sheet.write_string(
            at,
            9,
            if row.get::<_, i64>(9)? == 0 {
                "Indexable"
            } else {
                "noindex"
            },
        )?;
        write_opt(sheet, at, 10, row.get(10)?)?;
        sheet.write_number(at, 11, row.get::<_, i64>(11)? as f64)?;
        n += 1;
    }
    Ok((n, dropped))
}

fn write_images(workbook: &mut Workbook, store: &Store) -> Result<u64, ExportError> {
    let total: i64 = store
        .conn()
        .query_row("SELECT count(*) FROM resources", [], |r| r.get(0))?;
    if total == 0 {
        // No sheet rather than an empty one. A workbook of five sheets, four
        // of them empty, reads as a broken export.
        return Ok(0);
    }

    let sheet = workbook.add_worksheet_with_constant_memory();
    sheet.set_name("Images")?;
    head(
        sheet,
        &[
            ("Image", 80.0),
            ("Status", 9.0),
            ("Size (kB)", 11.0),
            ("Type", 16.0),
            ("Findings", 10.0),
        ],
    )?;

    let mut stmt = store.conn().prepare(
        "SELECT r.url, r.status, r.content_length, r.content_type, \
                (SELECT count(*) FROM issues i WHERE i.url = r.url) \
         FROM resources r ORDER BY r.url",
    )?;
    let mut rows = stmt.query([])?;
    let mut n = 0u64;
    while let Some(row) = rows.next()? {
        if n >= MAX_ROWS {
            break;
        }
        let at = (n + 1) as u32;
        sheet.write_string(at, 0, row.get::<_, String>(0)?)?;
        sheet.write_number(at, 1, row.get::<_, i64>(1)? as f64)?;
        // Blank, not zero, when the server declared no length — the same
        // distinction the grid draws as "Not declared".
        match row.get::<_, Option<i64>>(2)? {
            Some(bytes) => sheet.write_number(at, 2, (bytes as f64 / 1024.0).round())?,
            None => sheet.write_blank(at, 2, &Format::default())?,
        };
        write_opt(sheet, at, 3, row.get(3)?)?;
        sheet.write_number(at, 4, row.get::<_, i64>(4)? as f64)?;
        n += 1;
    }
    Ok(n)
}

fn write_sitemap(workbook: &mut Workbook, store: &Store) -> Result<u64, ExportError> {
    let total: i64 = store
        .conn()
        .query_row("SELECT count(*) FROM sitemap_urls", [], |r| r.get(0))?;
    if total == 0 {
        return Ok(0);
    }

    let sheet = workbook.add_worksheet_with_constant_memory();
    sheet.set_name("Sitemap")?;
    head(
        sheet,
        &[
            ("URL in the sitemap", 80.0),
            ("In the crawl", 14.0),
            ("Listed in", 60.0),
        ],
    )?;

    let mut stmt = store.conn().prepare(
        "SELECT s.url, p.status, s.source FROM sitemap_urls s \
         LEFT JOIN pages p ON p.url = s.url ORDER BY s.url",
    )?;
    let mut rows = stmt.query([])?;
    let mut n = 0u64;
    while let Some(row) = rows.next()? {
        if n >= MAX_ROWS {
            break;
        }
        let at = (n + 1) as u32;
        sheet.write_string(at, 0, row.get::<_, String>(0)?)?;
        // The finding, spelled out: a URL the site advertises that no link
        // reaches. A blank cell here would read as missing data.
        match row.get::<_, Option<i64>>(1)? {
            Some(status) => sheet.write_number(at, 1, status as f64)?,
            None => sheet.write_string(at, 1, "Not reached")?,
        };
        sheet.write_string(at, 2, row.get::<_, String>(2)?)?;
        n += 1;
    }
    Ok(n)
}
