//! The workbook: what lands in it, and what it says when it cannot hold it all.

use pounce_core::CrawlUrl;
use pounce_export::export_workbook;
use pounce_parse::{BodyKind, MetaRobots, PageRecord};
use pounce_store::{FilterSpec, SortColumn, SortDirection, SortSpec, Store, Writer};
use std::collections::BTreeMap;

fn record(url: &str, title: Option<&str>) -> PageRecord {
    PageRecord {
        url: CrawlUrl::parse(url).unwrap(),
        status: 200,
        depth: 1,
        size: 2_048,
        truncated: false,
        content_type: Some("text/html".into()),
        charset: None,
        kind: BodyKind::Html,
        content_type_mismatch: false,
        elapsed_ms: 12,
        time_to_headers_ms: 4,
        redirect_chain: vec![],
        title: title.map(str::to_string),
        title_count: 1,
        meta_description: None,
        h1: vec!["A heading".into()],
        h2: vec![],
        canonical: None,
        canonical_url: None,
        meta_robots: MetaRobots::default(),
        hreflang: vec![],
        open_graph: vec![],
        links: vec![],
        images: vec![],
        word_count: 300,
        body_hash: None,
    }
}

/// A store with one of everything the workbook has a sheet for.
fn seeded() -> Store {
    let mut store = Store::in_memory().unwrap();
    {
        let mut writer = Writer::new(&mut store);
        for (path, title) in [("a", Some("First")), ("b", None), ("c", Some(""))] {
            let record = record(&format!("https://example.com/{path}"), title);
            writer.push(&record).unwrap();
            let id = writer.page_id(&record.url.to_string()).unwrap();
            if path == "b" {
                writer
                    .issues(
                        &record.url.to_string(),
                        id,
                        &[("title.missing", "critical", Some("no <title> at all"))],
                    )
                    .unwrap();
            }
        }
        writer
            .resource(
                &CrawlUrl::parse("https://example.com/logo.png").unwrap(),
                200,
                Some(4_096),
                Some("image/png"),
            )
            .unwrap();
        writer.flush().unwrap();
    }
    store
        .put_sitemap_urls(
            "https://example.com/sitemap.xml",
            &[
                "https://example.com/a".to_string(),
                // Listed and never crawled: the finding the sheet exists for.
                "https://example.com/ghost".to_string(),
            ],
        )
        .unwrap();
    store
}

fn rules() -> BTreeMap<String, String> {
    BTreeMap::from([(
        "title.missing".to_string(),
        "The page has no title.".to_string(),
    )])
}

#[test]
fn every_sheet_holds_what_it_counted() {
    let store = seeded();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.xlsx");
    let filters = FilterSpec::new();
    let sort = SortSpec::new(&filters, SortColumn::Url, SortDirection::Asc).unwrap();

    let summary = export_workbook(&store, &filters, &sort, &rules(), &path).unwrap();
    assert_eq!(summary.pages, 3);
    assert_eq!(summary.issues, 1);
    assert_eq!(summary.images, 1);
    assert_eq!(summary.sitemap, 2);
    assert_eq!(summary.pages_not_written, 0);

    // A real workbook is a zip. Checking the magic bytes is not checking the
    // formatting, but it does catch the failure that matters — a file written
    // that Excel refuses to open.
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(&bytes[..2], b"PK", "not a zip, so not a workbook");
    assert!(bytes.len() > 2_000, "suspiciously small for five sheets");
}

#[test]
fn the_workbook_carries_the_view_it_was_exported_from() {
    // Same rule as the CSV path: Export writes what is on screen, filters and
    // all, or the button means something different from what it says.
    use pounce_store::{Comparison, Filter};
    let store = seeded();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("filtered.xlsx");
    let filters = FilterSpec::new().with(Filter::Status(Comparison::Ge, 400));
    let sort = SortSpec::new(&filters, SortColumn::Url, SortDirection::Asc).unwrap();

    let summary = export_workbook(&store, &filters, &sort, &rules(), &path).unwrap();
    assert_eq!(summary.pages, 0, "no page in this crawl is 4xx or worse");
    // The other sheets are the whole crawl on purpose: a client asking "what
    // is wrong with my site" is not asking about the filter someone left on.
    assert_eq!(summary.issues, 1);
    assert_eq!(summary.sitemap, 2);
}

#[test]
fn a_crawl_with_no_images_gets_no_images_sheet() {
    // Five sheets, four of them empty, reads as a broken export.
    let mut store = Store::in_memory().unwrap();
    {
        let mut writer = Writer::new(&mut store);
        writer
            .push(&record("https://example.com/only", None))
            .unwrap();
        writer.flush().unwrap();
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bare.xlsx");
    let filters = FilterSpec::new();
    let sort = SortSpec::new(&filters, SortColumn::Url, SortDirection::Asc).unwrap();

    let summary = export_workbook(&store, &filters, &sort, &rules(), &path).unwrap();
    assert_eq!(summary.images, 0);
    assert_eq!(summary.sitemap, 0);
    assert_eq!(summary.pages, 1);
}

// ---- the PDF report -------------------------------------------------------

#[test]
fn the_report_is_a_pdf_and_counts_what_it_found() {
    use pounce_export::{ReportMeta, export_report};
    let store = seeded();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audit.pdf");
    let sentences = BTreeMap::from([(
        "title.missing".to_string(),
        (
            "The page has no title.".to_string(),
            "Add one naming the page.".to_string(),
        ),
    )]);

    let summary = export_report(
        &store,
        &sentences,
        &ReportMeta {
            date: "29 August 2026",
            file: "seeded.pounce",
        },
        &path,
    )
    .unwrap();
    assert_eq!(summary.findings, 1);
    assert_eq!(summary.pages, 3);

    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(&bytes[..4], b"%PDF", "not a PDF");
    assert!(bytes.len() > 1_000, "suspiciously small for a report");
}

#[test]
fn a_crawl_with_nothing_wrong_still_produces_a_report() {
    // The empty case is the one a tool gets wrong, and it is the report an
    // agency most wants to send: "we looked, and here is what we looked at."
    use pounce_export::{ReportMeta, export_report};
    let mut store = Store::in_memory().unwrap();
    {
        let mut writer = Writer::new(&mut store);
        writer
            .push(&record("https://example.com/fine", Some("A title")))
            .unwrap();
        writer.flush().unwrap();
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("clean.pdf");
    let summary = export_report(
        &store,
        &BTreeMap::new(),
        &ReportMeta {
            date: "29 August 2026",
            file: "clean.pounce",
        },
        &path,
    )
    .unwrap();
    assert_eq!(summary.findings, 0);
    assert_eq!(&std::fs::read(&path).unwrap()[..4], b"%PDF");
}
