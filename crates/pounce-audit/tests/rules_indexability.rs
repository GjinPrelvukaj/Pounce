//! Indexability batch: every rule with a triggering and a non-triggering
//! fixture.

use pounce_audit::rules::indexability::{
    BlockedButLinked, CanonicalChain, CanonicalMismatch, CanonicalToNon200, Noindex,
    ROBOTS_DENIED_PREFIX,
};
use pounce_audit::{Issue, PageRule, Registry, SiteRule};
use pounce_core::CrawlUrl;
use pounce_parse::{BodyKind, Link, MetaRobots, PageRecord};
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
        h1: vec!["Heading".into()],
        h2: vec![],
        canonical: None,
        canonical_url: None,
        meta_robots: MetaRobots::default(),
        hreflang: vec![],
        open_graph: vec![],
        links: vec![],
        images: vec![],
        word_count: 500,
        body_hash: None,
    }
}

fn run(rule: &dyn PageRule, record: &PageRecord) -> Vec<Issue> {
    let mut out = Vec::new();
    rule.check(record, &mut out);
    out
}

/// Canonical pointing at itself — the ordinary, correct case.
fn self_canonical(url: &str) -> PageRecord {
    let mut p = page(url);
    p.canonical = Some(url.into());
    p.canonical_url = CrawlUrl::parse(url).ok();
    p
}

// ---- indexability.noindex ----------------------------------------------

#[test]
fn a_noindex_page_triggers_the_rule() {
    let mut p = page("https://e.com/a");
    p.meta_robots = MetaRobots::parse("noindex");
    assert_eq!(run(&Noindex, &p).len(), 1);
}

#[test]
fn an_indexable_page_does_not_trigger_noindex() {
    assert!(run(&Noindex, &page("https://e.com/a")).is_empty());
    // nofollow alone is a different directive and not this rule's business.
    let mut nofollow = page("https://e.com/a");
    nofollow.meta_robots = MetaRobots::parse("nofollow");
    assert!(run(&Noindex, &nofollow).is_empty());
}

// ---- indexability.canonical-elsewhere ----------------------------------

#[test]
fn a_canonical_pointing_away_triggers_the_mismatch_rule() {
    let mut p = page("https://e.com/dupe");
    p.canonical = Some("https://e.com/original".into());
    p.canonical_url = CrawlUrl::parse("https://e.com/original").ok();
    let issues = run(&CanonicalMismatch, &p);
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].detail.as_deref(), Some("https://e.com/original"));
}

#[test]
fn a_self_referencing_or_absent_canonical_does_not_trigger_mismatch() {
    assert!(run(&CanonicalMismatch, &self_canonical("https://e.com/a")).is_empty());
    // No canonical at all is not a mismatch — there is nothing to disagree.
    assert!(run(&CanonicalMismatch, &page("https://e.com/a")).is_empty());
}

// ---- site-rule fixtures -------------------------------------------------

fn seed(store: &mut Store, pages: &[PageRecord]) {
    let mut writer = Writer::with_batch_size(store, pages.len().max(1));
    for p in pages {
        writer.push(p).unwrap();
    }
    writer.flush().unwrap();
}

fn canonical_to(url: &str, target: &str) -> PageRecord {
    let mut p = page(url);
    p.canonical = Some(target.into());
    p.canonical_url = CrawlUrl::parse(target).ok();
    p
}

// ---- indexability.canonical-non-200 ------------------------------------

#[test]
fn a_canonical_pointing_at_a_404_triggers() {
    let mut store = Store::in_memory().unwrap();
    let mut gone = page("https://e.com/gone");
    gone.status = 404;
    seed(
        &mut store,
        &[canonical_to("https://e.com/a", "https://e.com/gone"), gone],
    );
    let found = CanonicalToNon200.check(&store).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, "https://e.com/a");
    assert_eq!(
        found[0].1.detail.as_deref(),
        Some("https://e.com/gone returns 404")
    );
}

#[test]
fn a_canonical_pointing_at_a_200_does_not_trigger() {
    let mut store = Store::in_memory().unwrap();
    seed(
        &mut store,
        &[
            canonical_to("https://e.com/a", "https://e.com/ok"),
            self_canonical("https://e.com/ok"),
        ],
    );
    assert!(CanonicalToNon200.check(&store).unwrap().is_empty());
}

// ---- indexability.canonical-chain --------------------------------------

#[test]
fn a_canonical_pointing_at_a_canonicalised_page_triggers_the_chain_rule() {
    let mut store = Store::in_memory().unwrap();
    seed(
        &mut store,
        &[
            canonical_to("https://e.com/a", "https://e.com/b"),
            canonical_to("https://e.com/b", "https://e.com/c"),
            self_canonical("https://e.com/c"),
        ],
    );
    let found = CanonicalChain.check(&store).unwrap();
    assert_eq!(found.len(), 1, "only /a starts a chain");
    assert_eq!(found[0].0, "https://e.com/a");
}

#[test]
fn a_canonical_landing_on_a_self_referencing_page_is_not_a_chain() {
    // The middle page canonicalising to itself terminates the chain, which is
    // exactly how canonicals are supposed to work.
    let mut store = Store::in_memory().unwrap();
    seed(
        &mut store,
        &[
            canonical_to("https://e.com/a", "https://e.com/b"),
            self_canonical("https://e.com/b"),
        ],
    );
    assert!(CanonicalChain.check(&store).unwrap().is_empty());
}

// ---- indexability.blocked-but-linked -----------------------------------

fn seed_blocked(store: &mut Store, blocked: &str, reason: &str, linked_from: &[&str]) {
    let mut writer = Writer::with_batch_size(store, 8);
    writer
        .discover(&[(CrawlUrl::parse(blocked).unwrap(), 1)])
        .unwrap();
    writer
        .fail(&CrawlUrl::parse(blocked).unwrap(), reason)
        .unwrap();
    for source in linked_from {
        let mut p = page(source);
        p.links = vec![Link {
            href: blocked.into(),
            target: CrawlUrl::parse(blocked).ok(),
            text: "link".into(),
            nofollow: false,
        }];
        writer.push(&p).unwrap();
    }
    writer.flush().unwrap();
}

#[test]
fn a_robots_blocked_url_that_is_linked_to_triggers() {
    let mut store = Store::in_memory().unwrap();
    // The real error's own Display, so the rule is matched against the message
    // the crawler actually produces.
    let real = pounce_http::fetch::FetchError::RobotsDenied(
        CrawlUrl::parse("https://e.com/private").unwrap(),
    )
    .to_string();
    seed_blocked(
        &mut store,
        "https://e.com/private",
        &real,
        &["https://e.com/a", "https://e.com/b"],
    );
    let found = BlockedButLinked.check(&store).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].1.detail.as_deref(), Some("linked from 2 page(s)"));
}

#[test]
fn a_blocked_url_nobody_links_to_does_not_trigger() {
    let mut store = Store::in_memory().unwrap();
    let real = pounce_http::fetch::FetchError::RobotsDenied(
        CrawlUrl::parse("https://e.com/private").unwrap(),
    )
    .to_string();
    seed_blocked(&mut store, "https://e.com/private", &real, &[]);
    assert!(BlockedButLinked.check(&store).unwrap().is_empty());

    // Nor does a failure that is not a robots denial, even when linked to.
    let mut other = Store::in_memory().unwrap();
    seed_blocked(
        &mut other,
        "https://e.com/down",
        "no response from https://e.com/down after 3 attempt(s): could not connect",
        &["https://e.com/a"],
    );
    assert!(BlockedButLinked.check(&other).unwrap().is_empty());
}

#[test]
fn the_robots_denied_prefix_still_matches_the_real_error() {
    // This rule matches a Display string, so a reworded error would silently
    // stop it firing. Pinned here against the real type rather than trusted.
    let real =
        pounce_http::fetch::FetchError::RobotsDenied(CrawlUrl::parse("https://e.com/x").unwrap())
            .to_string();
    assert!(
        real.starts_with(ROBOTS_DENIED_PREFIX),
        "FetchError::RobotsDenied now reads {real:?}, which no longer starts with \
         {ROBOTS_DENIED_PREFIX:?} — indexability.blocked-but-linked has stopped working"
    );
    // And the unreadable-robots error must NOT match: a host that is down is
    // not a site that blocked us, and T1.6a exists to keep them apart.
    let unreadable = pounce_http::fetch::FetchError::RobotsUnreadable {
        url: CrawlUrl::parse("https://e.com/x").unwrap(),
        reason: "could not connect".into(),
    }
    .to_string();
    assert!(!unreadable.starts_with(ROBOTS_DENIED_PREFIX));
}

// ---- the batch ----------------------------------------------------------

#[test]
fn the_batch_registers_five_rules() {
    let mut reg = Registry::new();
    pounce_audit::rules::indexability::register(&mut reg).unwrap();
    assert_eq!(reg.len(), 5);
    assert_eq!(reg.site_rules().len(), 3);
}
