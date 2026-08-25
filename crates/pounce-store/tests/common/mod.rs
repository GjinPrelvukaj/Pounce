#![allow(dead_code)] // each test binary uses a different part of this

//! A seeded store the query tests can ask real questions of.
//!
//! Shaped like a crawl rather than like a fixture: a status mix, a few
//! non-HTML bodies, some noindex, word counts that vary, and issues on a third
//! of the pages. The probe's all-200 database made every filter maximally
//! unselective, which is the pessimistic case worth measuring but the wrong
//! one to write correctness tests against.

use pounce_core::CrawlUrl;
use pounce_parse::{BodyKind, MetaRobots, PageRecord};
use pounce_store::{Store, Writer};

/// Page `i`'s URL. Every fifth page lives under `/blog/`, so a substring
/// filter has something to find that is not "everything".
pub fn url_of(i: u64) -> String {
    if i.is_multiple_of(5) {
        format!("http://e.com/blog/post-{i}")
    } else {
        format!("http://e.com/section/word-{i}")
    }
}

/// The status page `i` was served with: ~90% 200s, then 404s and 500s.
pub fn status_of(i: u64) -> u16 {
    match i {
        _ if i.is_multiple_of(50) => 500,
        _ if i.is_multiple_of(20) => 404,
        _ => 200,
    }
}

pub fn kind_of(i: u64) -> BodyKind {
    match i {
        _ if i.is_multiple_of(25) => BodyKind::Pdf,
        _ if i.is_multiple_of(40) => BodyKind::Image,
        _ => BodyKind::Html,
    }
}

pub fn word_count_of(i: u64) -> u32 {
    100 + (i * 37 % 900) as u32
}

pub fn noindex_of(i: u64) -> bool {
    i.is_multiple_of(10)
}

/// Pages carrying an issue, and which rule it is.
pub fn issue_of(i: u64) -> Option<(&'static str, &'static str)> {
    match i % 3 {
        0 => Some(("title.missing", "critical")),
        1 => Some(("description.missing", "warning")),
        _ => None,
    }
}

/// A record for an arbitrary URL, for tests that need a shape the numbered
/// seeder does not produce.
pub fn record_for(url: &str) -> PageRecord {
    PageRecord {
        url: CrawlUrl::parse(url).unwrap(),
        ..record(1)
    }
}

fn record(i: u64) -> PageRecord {
    PageRecord {
        url: CrawlUrl::parse(&url_of(i)).unwrap(),
        status: status_of(i),
        depth: (i % 5) as u16,
        size: 18_000 + (i % 4_000) as usize,
        truncated: false,
        content_type: Some("text/html".into()),
        charset: Some("utf-8".into()),
        kind: kind_of(i),
        content_type_mismatch: false,
        elapsed_ms: (7 + i % 40) as u32,
        time_to_headers_ms: 3,
        redirect_chain: vec![],
        title: Some(format!("Title number {i} with a few more words")),
        title_count: 1,
        meta_description: Some(format!("Description {i}, a sentence of summary.")),
        h1: vec![format!("A heading naming page {i}'s subject")],
        h2: vec![],
        canonical: None,
        canonical_url: None,
        meta_robots: MetaRobots {
            noindex: noindex_of(i),
            ..MetaRobots::default()
        },
        hreflang: vec![],
        open_graph: vec![],
        links: vec![],
        images: vec![],
        word_count: word_count_of(i),
        body_hash: Some(i.wrapping_mul(0x9E37_79B9_7F4A_7C15)),
    }
}

/// Seeds `pages` pages, ids `1..=pages`, and builds the read-path indices as a
/// finished crawl would.
pub fn seed(store: &mut Store, pages: u64) {
    seed_pages_only(store, pages);
    store.build_query_indices().unwrap();
}

/// Seeds `pages` pages writing exactly `per_page` findings on every one.
///
/// `seed_pages_only` writes one finding to a *subset*, which is ~0.67 per page
/// — a third of what a real crawl of the bench fixture produces (2.24). That
/// gap is the untested variable in
/// `docs/benchmarks/2026-08-25-rule-overhead-at-500k.md`: every store-side
/// figure there is scaled rather than measured at the crawl's density, and
/// scaling only holds if the cost per issue is linear in density.
pub fn seed_pages_with_issue_density(store: &mut Store, pages: u64, per_page: usize, batch: usize) {
    let mut writer = Writer::with_batch_size(store, batch);
    for i in 1..=pages {
        let record = record(i);
        let page_id = writer.push(&record).unwrap();
        if per_page > 0 {
            // The same rule id repeated: this prices the write, not the
            // registry, and a finding's cost does not depend on which rule
            // found it.
            let findings: Vec<(&'static str, &'static str, Option<&str>)> = (0..per_page)
                .map(|_| ("title.missing", "critical", None))
                .collect();
            writer
                .issues(&record.url.to_string(), Some(page_id), &findings)
                .unwrap();
        }
    }
    writer.flush().unwrap();
}

/// The same rows with no findings at all, so a harness can price what writing
/// issues — and flagging their pages — costs on the write path.
pub fn seed_pages_without_issues(store: &mut Store, pages: u64) {
    let mut writer = Writer::with_batch_size(store, 5_000);
    for i in 1..=pages {
        writer.push(&record(i)).unwrap();
    }
    writer.flush().unwrap();
}

/// The rows without the post-crawl index build, so a harness can time the two
/// separately. Streams: one record is built and dropped per page, so a million
/// rows cost a million rows of disk and nothing of memory.
pub fn seed_pages_only(store: &mut Store, pages: u64) {
    {
        let mut writer = Writer::with_batch_size(store, 5_000);
        for i in 1..=pages {
            let record = record(i);
            let page_id = writer.push(&record).unwrap();
            if let Some((rule, severity)) = issue_of(i) {
                writer
                    .issues(
                        &record.url.to_string(),
                        Some(page_id),
                        &[(rule, severity, None)],
                    )
                    .unwrap();
            }
        }
        writer.flush().unwrap();
    }
}

/// A seeded in-memory store. Disk is the product's rule; a test asking one
/// question of fifty rows is not the product.
pub fn seeded(pages: u64) -> Store {
    let mut store = Store::in_memory().unwrap();
    seed(&mut store, pages);
    store
}

/// How many pages match a `FilterSpec`, straight through its compiled SQL.
pub fn count_matching(store: &Store, spec: &pounce_store::FilterSpec) -> i64 {
    let (where_sql, params) = spec.compile();
    store
        .conn()
        .query_row(
            &format!("SELECT count(*) FROM pages p {where_sql}"),
            rusqlite::params_from_iter(params.iter()),
            |r| r.get(0),
        )
        .unwrap()
}
