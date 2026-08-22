//! Titles batch: every rule with a triggering and a non-triggering fixture.

use pounce_audit::rules::titles::{
    DuplicateTitle, MissingTitle, MultipleTitles, TitleTooLong, TitleTooShort,
};
use pounce_audit::{Issue, PageRule, Registry, SiteRule};
use pounce_core::CrawlUrl;
use pounce_parse::{BodyKind, MetaRobots, PageRecord};
use pounce_store::{Store, Writer};

/// 46 characters — comfortably inside both bounds.
const GOOD: &str = "A perfectly reasonable title for one page here";

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
        title: Some(GOOD.into()),
        title_count: 1,
        meta_description: None,
        h1: vec![],
        h2: vec![],
        canonical: None,
        canonical_url: None,
        meta_robots: MetaRobots::default(),
        hreflang: vec![],
        open_graph: vec![],
        links: vec![],
        images: vec![],
        word_count: 10,
        body_hash: None,
    }
}

fn run(rule: &dyn PageRule, record: &PageRecord) -> Vec<Issue> {
    let mut out = Vec::new();
    rule.check(record, &mut out);
    out
}

#[test]
fn the_baseline_fixture_trips_no_title_rule() {
    // Guards every "does not trigger" test below: if GOOD were itself out of
    // bounds, those tests would pass for the wrong reason.
    let p = page("https://e.com/a");
    assert_eq!(GOOD.chars().count(), 46);
    for rule in [
        &MissingTitle as &dyn PageRule,
        &TitleTooLong,
        &TitleTooShort,
        &MultipleTitles,
    ] {
        assert!(
            run(rule, &p).is_empty(),
            "{} fired on the baseline",
            rule.meta().id
        );
    }
}

// ---- title.missing ------------------------------------------------------

#[test]
fn a_page_with_no_title_triggers_missing() {
    let mut p = page("https://e.com/a");
    p.title = None;
    assert_eq!(run(&MissingTitle, &p).len(), 1);
}

#[test]
fn an_empty_title_is_not_missing() {
    // Absent is not empty. `<title></title>` is markup that has a title and
    // left it blank, which title.too-short reports; merging them would lose
    // which of the two mistakes was actually made.
    let mut p = page("https://e.com/a");
    p.title = Some(String::new());
    assert!(run(&MissingTitle, &p).is_empty());
    assert_eq!(run(&TitleTooShort, &p).len(), 1, "but it is too short");
}

// ---- title.too-long -----------------------------------------------------

#[test]
fn a_sixty_one_character_title_is_too_long() {
    let mut p = page("https://e.com/a");
    p.title = Some("x".repeat(61));
    let issues = run(&TitleTooLong, &p);
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].detail.as_deref(), Some("61 characters"));
}

#[test]
fn a_sixty_character_title_is_not_too_long() {
    // The boundary is the rule. Off by one here flags a title that fits.
    let mut p = page("https://e.com/a");
    p.title = Some("x".repeat(60));
    assert!(run(&TitleTooLong, &p).is_empty());
}

#[test]
fn length_is_counted_in_characters_not_bytes() {
    // 60 accented characters are 120 bytes. A byte count would report this as
    // too long, and would do so only for non-ASCII sites.
    let mut p = page("https://e.com/a");
    p.title = Some("é".repeat(60));
    assert_eq!(p.title.as_deref().unwrap().len(), 120, "120 bytes");
    assert!(
        run(&TitleTooLong, &p).is_empty(),
        "but 60 characters, so it fits"
    );
}

// ---- title.too-short ----------------------------------------------------

#[test]
fn a_twenty_nine_character_title_is_too_short() {
    let mut p = page("https://e.com/a");
    p.title = Some("x".repeat(29));
    assert_eq!(run(&TitleTooShort, &p).len(), 1);
}

#[test]
fn a_thirty_character_title_is_not_too_short() {
    let mut p = page("https://e.com/a");
    p.title = Some("x".repeat(30));
    assert!(run(&TitleTooShort, &p).is_empty());
    // And a missing title is missing, not short — one mistake, one finding.
    p.title = None;
    assert!(run(&TitleTooShort, &p).is_empty());
}

// ---- title.multiple -----------------------------------------------------

#[test]
fn two_title_elements_trigger_multiple() {
    let mut p = page("https://e.com/a");
    p.title_count = 2;
    let issues = run(&MultipleTitles, &p);
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].detail.as_deref(), Some("2 <title> elements"));
}

#[test]
fn one_or_zero_title_elements_do_not_trigger_multiple() {
    let mut p = page("https://e.com/a");
    p.title_count = 1;
    assert!(run(&MultipleTitles, &p).is_empty());
    p.title_count = 0;
    p.title = None;
    assert!(run(&MultipleTitles, &p).is_empty());
}

// ---- title.duplicate ----------------------------------------------------

fn seed(store: &mut Store, rows: &[(&str, Option<&str>)]) {
    let mut writer = Writer::with_batch_size(store, rows.len().max(1));
    for (url, title) in rows {
        let mut record = page(url);
        record.title = title.map(str::to_string);
        writer.push(&record).unwrap();
    }
    writer.flush().unwrap();
}

#[test]
fn two_pages_sharing_a_title_both_trigger_duplicate() {
    let mut store = Store::in_memory().unwrap();
    seed(
        &mut store,
        &[
            ("https://e.com/a", Some("Shared")),
            ("https://e.com/b", Some("Shared")),
            ("https://e.com/c", Some("Unique")),
        ],
    );
    let found = DuplicateTitle.check(&store).unwrap();
    assert_eq!(
        found.len(),
        2,
        "both sharers are findings, not just the later one"
    );
    assert!(!found.iter().any(|(url, _)| url.ends_with("/c")));
    assert_eq!(found[0].1.detail.as_deref(), Some("Shared"));
}

#[test]
fn distinct_titles_and_absent_titles_do_not_trigger_duplicate() {
    let mut store = Store::in_memory().unwrap();
    seed(
        &mut store,
        &[
            ("https://e.com/a", Some("First title here")),
            ("https://e.com/b", Some("Second title here")),
            // Two pages with no title are two title.missing findings, not one
            // shared title.
            ("https://e.com/c", None),
            ("https://e.com/d", None),
        ],
    );
    assert!(DuplicateTitle.check(&store).unwrap().is_empty());
}

// ---- the batch ----------------------------------------------------------

#[test]
fn the_batch_registers_five_rules() {
    let mut reg = Registry::new();
    pounce_audit::rules::titles::register(&mut reg).unwrap();
    assert_eq!(reg.len(), 5);
    assert_eq!(
        reg.site_rules().len(),
        1,
        "only duplicate needs the whole crawl"
    );
}

#[test]
fn all_shipped_rules_register_together_within_the_cap() {
    let mut reg = Registry::new();
    pounce_audit::register_all(&mut reg).unwrap();
    assert_eq!(reg.len(), 10, "two batches of five");
}

// ---- non-HTML bodies ----------------------------------------------------

#[test]
fn markup_rules_stay_silent_on_a_body_that_has_no_markup() {
    // Found end to end: title.missing fired on /sitemap.xml, a file working
    // exactly as intended. A PDF has no <title> either, and saying so is a
    // false positive rather than a finding.
    let mut reg = Registry::new();
    pounce_audit::rules::titles::register(&mut reg).unwrap();

    let mut sitemap = page("https://e.com/sitemap.xml");
    sitemap.kind = BodyKind::Other;
    sitemap.title = None;
    sitemap.title_count = 0;
    assert!(
        reg.run_page(&sitemap).is_empty(),
        "XML has no title to miss"
    );

    for kind in [BodyKind::Pdf, BodyKind::Image, BodyKind::Undeclared] {
        let mut other = page("https://e.com/f");
        other.kind = kind;
        other.title = None;
        assert!(reg.run_page(&other).is_empty(), "{kind:?}");
    }

    // The same record as HTML is a real finding.
    let mut html = page("https://e.com/a");
    html.kind = BodyKind::Html;
    html.title = None;
    assert_eq!(reg.run_page(&html).len(), 1);
}

#[test]
fn response_rules_still_apply_to_non_html() {
    // The opt-out has to work in the other direction: a 404 PDF is still a 404,
    // and defaulting every rule to HTML-only would have silenced it.
    let mut reg = Registry::new();
    pounce_audit::rules::response::register(&mut reg).unwrap();

    let mut pdf = page("https://e.com/brochure.pdf");
    pdf.kind = BodyKind::Pdf;
    pdf.status = 404;
    let issues = reg.run_page(&pdf);
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].rule_id, "response.4xx");
}
