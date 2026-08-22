//! Descriptions batch: every rule with a triggering and a non-triggering
//! fixture.

use pounce_audit::rules::descriptions::{
    DescriptionTooLong, DescriptionTooShort, DuplicateDescription, MissingDescription,
    TruncatedEntity,
};
use pounce_audit::{Issue, PageRule, Registry, SiteRule};
use pounce_core::CrawlUrl;
use pounce_parse::{BodyKind, MetaRobots, PageRecord};
use pounce_store::{Store, Writer};

/// 101 characters — inside both bounds, no trailing ampersand.
const GOOD: &str = "A description of exactly the right sort of length for a page that wants to explain itself well.......";

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
        meta_description: Some(GOOD.into()),
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

fn with(desc: Option<&str>) -> PageRecord {
    let mut p = page("https://e.com/a");
    p.meta_description = desc.map(str::to_string);
    p
}

#[test]
fn the_baseline_fixture_trips_no_description_rule() {
    assert_eq!(GOOD.chars().count(), 101);
    let p = page("https://e.com/a");
    for rule in [
        &MissingDescription as &dyn PageRule,
        &DescriptionTooLong,
        &DescriptionTooShort,
        &TruncatedEntity,
    ] {
        assert!(
            run(rule, &p).is_empty(),
            "{} fired on the baseline",
            rule.meta().id
        );
    }
}

// ---- description.missing ------------------------------------------------

#[test]
fn a_page_with_no_description_triggers_missing() {
    assert_eq!(run(&MissingDescription, &with(None)).len(), 1);
}

#[test]
fn an_empty_description_is_not_missing() {
    let p = with(Some(""));
    assert!(run(&MissingDescription, &p).is_empty());
    assert_eq!(
        run(&DescriptionTooShort, &p).len(),
        1,
        "but it is too short"
    );
}

// ---- description.too-long / too-short -----------------------------------

#[test]
fn a_161_character_description_is_too_long() {
    let issues = run(&DescriptionTooLong, &with(Some(&"x".repeat(161))));
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].detail.as_deref(), Some("161 characters"));
}

#[test]
fn a_160_character_description_is_not_too_long() {
    assert!(run(&DescriptionTooLong, &with(Some(&"x".repeat(160)))).is_empty());
    // Accents must not count double: 160 of them are 320 bytes.
    assert!(run(&DescriptionTooLong, &with(Some(&"é".repeat(160)))).is_empty());
}

#[test]
fn a_69_character_description_is_too_short() {
    assert_eq!(
        run(&DescriptionTooShort, &with(Some(&"x".repeat(69)))).len(),
        1
    );
}

#[test]
fn a_70_character_description_is_not_too_short() {
    assert!(run(&DescriptionTooShort, &with(Some(&"x".repeat(70)))).is_empty());
    assert!(
        run(&DescriptionTooShort, &with(None)).is_empty(),
        "missing is not short"
    );
}

// ---- description.truncated-entity ---------------------------------------

#[test]
fn a_description_ending_in_a_half_written_entity_triggers() {
    for (text, fragment) in [
        ("Read more about our products and servi&hellip", "&hellip"),
        ("Everything you need to know abou&amp", "&amp"),
        ("A description cut mid-numeric-reference&#82", "&#82"),
        ("A description cut mid-hex-reference&#x1F6", "&#x1F6"),
    ] {
        let issues = run(&TruncatedEntity, &with(Some(text)));
        assert_eq!(issues.len(), 1, "{text:?}");
        assert_eq!(issues[0].detail.as_deref(), Some(fragment), "{text:?}");
    }
}

#[test]
fn an_ordinary_ampersand_does_not_trigger_a_truncated_entity() {
    // The false positives this rule must not produce. `Q&A` ends in an
    // ampersand and a letter and is not truncated; matching "any & then
    // letters" would flag it, which is why the check is against known entity
    // names rather than against shape alone.
    for text in [
        "Everything about Q&A",
        "A guide to Ben & Jerry",
        "Fish & Chips and other British food traditions explained",
        "A complete entity is decoded before we see it, so &amp; is just &",
        GOOD,
    ] {
        assert!(
            run(&TruncatedEntity, &with(Some(text))).is_empty(),
            "{text:?}"
        );
    }
}

// ---- description.duplicate ----------------------------------------------

fn seed(store: &mut Store, rows: &[(&str, Option<&str>)]) {
    let mut writer = Writer::with_batch_size(store, rows.len().max(1));
    for (url, desc) in rows {
        let mut record = page(url);
        record.meta_description = desc.map(str::to_string);
        writer.push(&record).unwrap();
    }
    writer.flush().unwrap();
}

#[test]
fn two_pages_sharing_a_description_both_trigger_duplicate() {
    let mut store = Store::in_memory().unwrap();
    seed(
        &mut store,
        &[
            ("https://e.com/a", Some("Shared boilerplate")),
            ("https://e.com/b", Some("Shared boilerplate")),
            ("https://e.com/c", Some("Its own description")),
        ],
    );
    let found = DuplicateDescription.check(&store).unwrap();
    assert_eq!(found.len(), 2);
    assert!(!found.iter().any(|(url, _)| url.ends_with("/c")));
}

#[test]
fn distinct_and_absent_descriptions_do_not_trigger_duplicate() {
    let mut store = Store::in_memory().unwrap();
    seed(
        &mut store,
        &[
            ("https://e.com/a", Some("One")),
            ("https://e.com/b", Some("Two")),
            ("https://e.com/c", None),
            ("https://e.com/d", None),
        ],
    );
    assert!(DuplicateDescription.check(&store).unwrap().is_empty());
}

// ---- the batch ----------------------------------------------------------

#[test]
fn the_batch_registers_five_rules_and_all_shipped_rules_fit_the_cap() {
    let mut reg = Registry::new();
    pounce_audit::rules::descriptions::register(&mut reg).unwrap();
    assert_eq!(reg.len(), 5);

    // The running total, pinned in exactly one place. Landing a batch is meant
    // to break this line — it is the reminder to check the count against the
    // 30-rule cap rather than drift past it.
    let mut all = Registry::new();
    pounce_audit::register_all(&mut all).unwrap();
    assert_eq!(all.len(), 25, "five batches of five");
    assert!(all.len() <= pounce_audit::MAX_RULES);
}

#[test]
fn description_rules_stay_silent_on_non_html() {
    let mut reg = Registry::new();
    pounce_audit::rules::descriptions::register(&mut reg).unwrap();
    let mut pdf = page("https://e.com/f.pdf");
    pdf.kind = BodyKind::Pdf;
    pdf.meta_description = None;
    assert!(
        reg.run_page(&pdf).is_empty(),
        "a PDF has no meta description to miss"
    );
}
