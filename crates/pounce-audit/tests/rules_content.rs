//! Content batch: every rule with a triggering and a non-triggering fixture.

use pounce_audit::rules::content::{DuplicateBody, EmptyH1, MissingH1, MultipleH1, ThinContent};
use pounce_audit::{Issue, PageRule, Registry, SiteRule};
use pounce_core::CrawlUrl;
use pounce_parse::{BodyKind, MetaRobots, PageRecord};
use pounce_store::{Store, Writer};

fn page(url: &str) -> PageRecord {
    PageRecord {
        url: CrawlUrl::parse(url).unwrap(),
        status: 200,
        depth: 1,
        size: 100,
        truncated: false,
        content_type: Some("text/html".into()),
        charset: Some("utf-8".into()),
        kind: BodyKind::Html,
        content_type_mismatch: false,
        elapsed_ms: 5,
        time_to_headers_ms: 2,
        redirect_chain: vec![],
        title: Some("A title of a perfectly acceptable length here".into()),
        title_count: 1,
        meta_description: None,
        h1: vec!["The page heading".into()],
        h2: vec![],
        canonical: None,
        canonical_url: None,
        meta_robots: MetaRobots::default(),
        hreflang: vec![],
        open_graph: vec![],
        links: vec![],
        images: vec![],
        word_count: 500,
        body_hash: Some(0x1234_5678),
    }
}

fn run(rule: &dyn PageRule, record: &PageRecord) -> Vec<Issue> {
    let mut out = Vec::new();
    rule.check(record, &mut out);
    out
}

#[test]
fn the_baseline_fixture_trips_no_content_rule() {
    let p = page("https://e.com/a");
    for rule in [
        &MissingH1 as &dyn PageRule,
        &MultipleH1,
        &EmptyH1,
        &ThinContent,
    ] {
        assert!(
            run(rule, &p).is_empty(),
            "{} fired on the baseline",
            rule.meta().id
        );
    }
}

// ---- content.missing-h1 -------------------------------------------------

#[test]
fn a_page_with_no_h1_triggers_missing() {
    let mut p = page("https://e.com/a");
    p.h1 = vec![];
    assert_eq!(run(&MissingH1, &p).len(), 1);
}

#[test]
fn an_empty_h1_is_present_not_missing() {
    // `<h1></h1>` is a heading that exists and says nothing. Reporting it as
    // missing would send someone looking for markup that is already there.
    let mut p = page("https://e.com/a");
    p.h1 = vec![String::new()];
    assert!(run(&MissingH1, &p).is_empty());
    assert_eq!(run(&EmptyH1, &p).len(), 1, "but it is empty");
}

// ---- content.multiple-h1 ------------------------------------------------

#[test]
fn two_h1_elements_trigger_multiple() {
    let mut p = page("https://e.com/a");
    p.h1 = vec!["First".into(), "Second".into()];
    let issues = run(&MultipleH1, &p);
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].detail.as_deref(), Some("2 <h1> elements"));
}

#[test]
fn one_or_zero_h1_elements_do_not_trigger_multiple() {
    assert!(run(&MultipleH1, &page("https://e.com/a")).is_empty());
    let mut none = page("https://e.com/a");
    none.h1 = vec![];
    assert!(
        run(&MultipleH1, &none).is_empty(),
        "no h1 is missing, not multiple"
    );
}

// ---- content.empty-h1 ---------------------------------------------------

#[test]
fn a_blank_h1_among_full_ones_still_triggers_empty() {
    let mut p = page("https://e.com/a");
    p.h1 = vec!["Real heading".into(), String::new()];
    assert_eq!(run(&EmptyH1, &p).len(), 1);
}

#[test]
fn h1_elements_with_text_do_not_trigger_empty() {
    assert!(run(&EmptyH1, &page("https://e.com/a")).is_empty());
    let mut none = page("https://e.com/a");
    none.h1 = vec![];
    assert!(run(&EmptyH1, &none).is_empty(), "no h1 is not an empty one");
}

// ---- content.thin -------------------------------------------------------

#[test]
fn a_199_word_page_is_thin() {
    let mut p = page("https://e.com/a");
    p.word_count = 199;
    let issues = run(&ThinContent, &p);
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].detail.as_deref(), Some("199 words"));
}

#[test]
fn a_200_word_page_is_not_thin() {
    let mut p = page("https://e.com/a");
    p.word_count = 200;
    assert!(run(&ThinContent, &p).is_empty());
}

// ---- content.duplicate-body ---------------------------------------------

fn seed(store: &mut Store, rows: &[(&str, Option<u64>)]) {
    let mut writer = Writer::with_batch_size(store, rows.len().max(1));
    for (url, hash) in rows {
        let mut record = page(url);
        record.body_hash = *hash;
        writer.push(&record).unwrap();
    }
    writer.flush().unwrap();
}

#[test]
fn two_pages_with_the_same_body_hash_both_trigger_duplicate() {
    let mut store = Store::in_memory().unwrap();
    seed(
        &mut store,
        &[
            ("https://e.com/a", Some(111)),
            ("https://e.com/b", Some(111)),
            ("https://e.com/c", Some(222)),
        ],
    );
    let found = DuplicateBody.check(&store).unwrap();
    assert_eq!(found.len(), 2);
    assert!(!found.iter().any(|(url, _)| url.ends_with("/c")));
}

#[test]
fn distinct_bodies_and_textless_pages_do_not_trigger_duplicate() {
    let mut store = Store::in_memory().unwrap();
    seed(
        &mut store,
        &[
            ("https://e.com/a", Some(1)),
            ("https://e.com/b", Some(2)),
            // Two pages with no text at all are not duplicates of each other:
            // there is nothing to compare, which is why the hash is NULL.
            ("https://e.com/c", None),
            ("https://e.com/d", None),
        ],
    );
    assert!(DuplicateBody.check(&store).unwrap().is_empty());
}

#[test]
fn a_hash_with_the_high_bit_set_still_matches_through_sql() {
    // body_hash is a u64 stored in a signed column. If the round trip were
    // lossy, duplicate detection would silently stop working for half of all
    // possible hashes — the half no small test fixture ever produces.
    let mut store = Store::in_memory().unwrap();
    let big = 0xFFFF_FFFF_FFFF_FF00_u64;
    seed(
        &mut store,
        &[
            ("https://e.com/a", Some(big)),
            ("https://e.com/b", Some(big)),
        ],
    );
    assert_eq!(DuplicateBody.check(&store).unwrap().len(), 2);
}

// ---- the batch ----------------------------------------------------------

#[test]
fn the_batch_registers_five_rules() {
    let mut reg = Registry::new();
    pounce_audit::rules::content::register(&mut reg).unwrap();
    assert_eq!(reg.len(), 5);
    assert_eq!(reg.site_rules().len(), 1);
}

#[test]
fn content_rules_stay_silent_on_non_html() {
    let mut reg = Registry::new();
    pounce_audit::rules::content::register(&mut reg).unwrap();
    let mut pdf = page("https://e.com/f.pdf");
    pdf.kind = BodyKind::Pdf;
    pdf.h1 = vec![];
    pdf.word_count = 0;
    assert!(reg.run_page(&pdf).is_empty(), "a PDF has no <h1> to miss");
}
