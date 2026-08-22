//! Media & links batch: every rule with a triggering and a non-triggering
//! fixture.

use pounce_audit::rules::media_links::{
    BrokenImage, BrokenInternalLink, MissingAlt, OrphanPage, OversizedImage,
};
use pounce_audit::{Issue, PageRule, Registry, SiteRule};
use pounce_core::CrawlUrl;
use pounce_parse::{BodyKind, Image, Link, MetaRobots, PageRecord};
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

fn img(src: &str, alt: Option<&str>) -> Image {
    Image {
        src: src.into(),
        alt: alt.map(Into::into),
    }
}

fn link(target: &str) -> Link {
    Link {
        href: target.into(),
        target: Some(CrawlUrl::parse(target).unwrap()),
        text: "somewhere".into(),
        nofollow: false,
    }
}

fn run(rule: &dyn PageRule, record: &PageRecord) -> Vec<Issue> {
    let mut out = Vec::new();
    rule.check(record, &mut out);
    out
}

/// Seeds pages with their status, depth, and outbound edges.
///
/// Depth is explicit because `links.orphan-page` reads it: depth 0 is the URL
/// the crawl was started from, which nothing on the site is expected to link to.
fn seed(store: &mut Store, rows: &[(&str, u16, u16, &[&str])]) {
    let mut writer = Writer::with_batch_size(store, rows.len().max(1));
    for (url, status, depth, targets) in rows {
        let mut record = page(url);
        record.status = *status;
        record.depth = *depth;
        record.links = targets.iter().map(|t| link(t)).collect();
        writer.push(&record).unwrap();
    }
    writer.flush().unwrap();
}

#[test]
fn the_baseline_fixture_trips_no_media_rule() {
    let mut p = page("https://e.com/a");
    p.images = vec![img("/logo.png", Some("The company logo"))];
    assert!(run(&MissingAlt, &p).is_empty());
}

// ---- media.missing-alt --------------------------------------------------

#[test]
fn an_image_with_no_alt_attribute_triggers_missing_alt() {
    let mut p = page("https://e.com/a");
    p.images = vec![
        img("/logo.png", Some("The company logo")),
        img("/hero.jpg", None),
    ];
    let issues = run(&MissingAlt, &p);
    assert_eq!(issues.len(), 1);
    assert_eq!(
        issues[0].detail.as_deref(),
        Some("1 of 2 images: /hero.jpg")
    );
}

#[test]
fn an_empty_alt_is_a_decision_and_does_not_trigger() {
    // alt="" is the documented way to mark an image decorative, so a screen
    // reader skips it. Reporting it would tell someone to undo the correct fix.
    let mut p = page("https://e.com/a");
    p.images = vec![img("/spacer.gif", Some(""))];
    assert!(run(&MissingAlt, &p).is_empty());
}

#[test]
fn a_page_with_no_images_does_not_trigger() {
    assert!(run(&MissingAlt, &page("https://e.com/a")).is_empty());
}

#[test]
fn several_missing_alts_are_one_finding_naming_the_first() {
    // One issue per page, not per image: a template that forgot alt produces
    // one defect repeated, and a row per image would bury every other finding.
    let mut p = page("https://e.com/a");
    p.images = vec![
        img("/a.png", None),
        img("/b.png", None),
        img("/c.png", None),
    ];
    let issues = run(&MissingAlt, &p);
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].detail.as_deref(), Some("3 of 3 images: /a.png"));
}

// ---- links.broken-internal ----------------------------------------------

#[test]
fn a_link_to_a_404_triggers_broken_internal() {
    let mut store = Store::in_memory().unwrap();
    seed(
        &mut store,
        &[
            ("https://e.com/a", 200, 0, &["https://e.com/gone"][..]),
            ("https://e.com/b", 200, 1, &["https://e.com/gone"][..]),
            ("https://e.com/gone", 404, 1, &[][..]),
        ],
    );
    let found = BrokenInternalLink.check(&store).unwrap();
    assert_eq!(
        found.len(),
        1,
        "one finding per broken target, not per edge"
    );
    assert_eq!(found[0].0, "https://e.com/gone");
    assert_eq!(
        found[0].1.detail.as_deref(),
        Some("404, linked from 2 page(s)")
    );
}

#[test]
fn a_5xx_target_is_also_broken() {
    let mut store = Store::in_memory().unwrap();
    seed(
        &mut store,
        &[
            ("https://e.com/a", 200, 0, &["https://e.com/down"][..]),
            ("https://e.com/down", 503, 1, &[][..]),
        ],
    );
    assert_eq!(BrokenInternalLink.check(&store).unwrap().len(), 1);
}

#[test]
fn links_to_working_and_to_uncrawled_pages_do_not_trigger() {
    let mut store = Store::in_memory().unwrap();
    seed(
        &mut store,
        &[
            // A 200 target, and an external one with no page row at all. An
            // absent row means "not crawled", not "broken" — treating it as
            // broken would flag every outbound link on the site.
            (
                "https://e.com/a",
                200,
                0,
                &["https://e.com/b", "https://other.com/x"][..],
            ),
            ("https://e.com/b", 200, 1, &[][..]),
        ],
    );
    assert!(BrokenInternalLink.check(&store).unwrap().is_empty());
}

#[test]
fn a_broken_page_nobody_links_to_does_not_trigger() {
    // response.4xx already reports the page itself. This rule exists to say
    // that links point at it, so with no inbound edge there is nothing to add.
    let mut store = Store::in_memory().unwrap();
    seed(&mut store, &[("https://e.com/gone", 404, 1, &[][..])]);
    assert!(BrokenInternalLink.check(&store).unwrap().is_empty());
}

// ---- links.orphan-page --------------------------------------------------

#[test]
fn a_page_nothing_links_to_triggers_orphan() {
    let mut store = Store::in_memory().unwrap();
    seed(
        &mut store,
        &[
            // /a is the seed (depth 0) and links only to /b. /lonely was
            // reached some other way — a redirect, a sitemap — and no page
            // points at it.
            ("https://e.com/a", 200, 0, &["https://e.com/b"][..]),
            ("https://e.com/b", 200, 1, &[][..]),
        ],
    );
    {
        let mut writer = Writer::with_batch_size(&mut store, 1);
        let mut lonely = page("https://e.com/lonely");
        lonely.depth = 2;
        writer.push(&lonely).unwrap();
        writer.flush().unwrap();
    }
    let found = OrphanPage.check(&store).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, "https://e.com/lonely");
}

#[test]
fn the_seed_is_not_an_orphan_and_linked_pages_are_not_either() {
    // The seed is entered by hand, so nothing on the site links to it and
    // saying so every crawl would be one guaranteed false finding per run.
    let mut store = Store::in_memory().unwrap();
    seed(
        &mut store,
        &[
            ("https://e.com/a", 200, 0, &["https://e.com/b"][..]),
            ("https://e.com/b", 200, 1, &[][..]),
        ],
    );
    assert!(OrphanPage.check(&store).unwrap().is_empty());
}

#[test]
fn a_page_reached_only_through_a_nofollow_link_is_still_linked() {
    // nofollow is a ranking hint, not an absence of a link. The page is
    // discoverable, so calling it an orphan would misdescribe the graph.
    let mut store = Store::in_memory().unwrap();
    {
        let mut writer = Writer::with_batch_size(&mut store, 2);
        let mut seedp = page("https://e.com/a");
        seedp.depth = 0;
        let mut l = link("https://e.com/b");
        l.nofollow = true;
        seedp.links = vec![l];
        writer.push(&seedp).unwrap();
        writer.push(&page("https://e.com/b")).unwrap();
        writer.flush().unwrap();
    }
    assert!(OrphanPage.check(&store).unwrap().is_empty());
}

// ---- media.broken-image and media.oversized-image -----------------------

/// Seeds the `resources` table an image `HEAD` pass fills.
fn seed_resources(store: &mut Store, rows: &[(&str, u16, Option<u64>)]) {
    let mut writer = Writer::with_batch_size(store, rows.len().max(1));
    for (url, status, length) in rows {
        writer
            .resource(
                &CrawlUrl::parse(url).unwrap(),
                *status,
                *length,
                Some("image/jpeg"),
            )
            .unwrap();
    }
    writer.flush().unwrap();
}

#[test]
fn a_404_image_triggers_broken_image() {
    let mut store = Store::in_memory().unwrap();
    seed_resources(
        &mut store,
        &[
            ("https://e.com/ok.jpg", 200, Some(1024)),
            ("https://e.com/gone.jpg", 404, Some(13)),
        ],
    );
    let found = BrokenImage.check(&store).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, "https://e.com/gone.jpg");
    assert_eq!(found[0].1.detail.as_deref(), Some("returns 404"));
}

#[test]
fn images_that_load_do_not_trigger_broken_image() {
    let mut store = Store::in_memory().unwrap();
    seed_resources(&mut store, &[("https://e.com/ok.jpg", 200, Some(1024))]);
    assert!(BrokenImage.check(&store).unwrap().is_empty());
}

#[test]
fn a_crawl_that_checked_no_images_finds_no_image_issues() {
    // `resources` is empty when the pass was off. Both rules must stay silent
    // rather than infer anything from the markup — an unchecked image is
    // unknown, and reporting unknown as broken would make every crawl without
    // the flag look catastrophic.
    let store = Store::in_memory().unwrap();
    assert!(BrokenImage.check(&store).unwrap().is_empty());
    assert!(OversizedImage.check(&store).unwrap().is_empty());
}

#[test]
fn an_image_over_a_hundred_kilobytes_triggers_oversized() {
    let mut store = Store::in_memory().unwrap();
    seed_resources(
        &mut store,
        &[("https://e.com/hero.jpg", 200, Some(300 * 1024))],
    );
    let found = OversizedImage.check(&store).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].1.detail.as_deref(), Some("300 KB"));
}

#[test]
fn an_image_exactly_at_the_threshold_is_not_oversized() {
    let mut store = Store::in_memory().unwrap();
    seed_resources(
        &mut store,
        &[
            ("https://e.com/at.jpg", 200, Some(100 * 1024)),
            ("https://e.com/over.jpg", 200, Some(100 * 1024 + 1)),
        ],
    );
    let found = OversizedImage.check(&store).unwrap();
    assert_eq!(found.len(), 1, "the boundary itself is fine");
    assert_eq!(found[0].0, "https://e.com/over.jpg");
}

#[test]
fn an_undeclared_length_is_unknown_rather_than_small() {
    // NULL means the server declared no Content-Length. Reading it as 0 would
    // silently exempt every chunked or streamed image from the rule.
    let mut store = Store::in_memory().unwrap();
    seed_resources(&mut store, &[("https://e.com/stream.jpg", 200, None)]);
    assert!(OversizedImage.check(&store).unwrap().is_empty());
}

#[test]
fn a_broken_image_is_not_also_reported_as_oversized() {
    // A 404's Content-Length describes the error page, not the image. Charging
    // one broken URL to two rules would double it in the summary.
    let mut store = Store::in_memory().unwrap();
    seed_resources(
        &mut store,
        &[("https://e.com/gone.jpg", 404, Some(500 * 1024))],
    );
    assert_eq!(BrokenImage.check(&store).unwrap().len(), 1);
    assert!(OversizedImage.check(&store).unwrap().is_empty());
}

// ---- the batch ----------------------------------------------------------

#[test]
fn the_batch_registers_five_rules_and_completes_the_thirty() {
    let mut reg = Registry::new();
    pounce_audit::rules::media_links::register(&mut reg).unwrap();
    assert_eq!(reg.len(), 5);
    assert_eq!(reg.site_rules().len(), 4);

    // The running total, pinned in exactly one place.
    let mut all = Registry::new();
    pounce_audit::register_all(&mut all).unwrap();
    assert_eq!(all.len(), 30, "six batches of five");
    assert_eq!(all.len(), pounce_audit::MAX_RULES, "the cap is now reached");
}

#[test]
fn a_thirty_first_rule_is_rejected() {
    // The cap stops being theoretical the moment the set is full. Asserted
    // against the real shipped set rather than a hand-built one, so it is the
    // product's count that is being held to 30.
    let mut all = Registry::new();
    pounce_audit::register_all(&mut all).unwrap();
    assert!(all.register_page(Box::new(MissingAlt)).is_err());
}

#[test]
fn media_rules_stay_silent_on_non_html() {
    let mut reg = Registry::new();
    pounce_audit::rules::media_links::register(&mut reg).unwrap();
    let mut pdf = page("https://e.com/f.pdf");
    pdf.kind = BodyKind::Pdf;
    pdf.images = vec![img("/x.png", None)];
    assert!(reg.run_page(&pdf).is_empty(), "a PDF has no <img> to fix");
}
