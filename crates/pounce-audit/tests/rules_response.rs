//! Response batch: every rule with a fixture that triggers it and one that
//! does not. Non-negotiable per the spec — it is what stops rule count
//! becoming rule debt.

use pounce_audit::rules::response::{
    ClientError, LongRedirectChain, MixedContent, RedirectLoop, ServerError,
};
use pounce_audit::{Issue, PageRule, Registry, SiteRule};
use pounce_core::CrawlUrl;
use pounce_parse::{BodyKind, Link, MetaRobots, PageRecord};
use pounce_store::{RedirectHop, Store, Writer};

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
        title: Some("A page".into()),
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

fn link(href: &str, base: &str) -> Link {
    let target = CrawlUrl::parse(base).unwrap().join(href).ok();
    Link {
        href: href.into(),
        target,
        text: "link".into(),
        nofollow: false,
    }
}

fn run(rule: &dyn PageRule, record: &PageRecord) -> Vec<Issue> {
    let mut out = Vec::new();
    rule.check(record, &mut out);
    out
}

// ---- response.4xx -------------------------------------------------------

#[test]
fn a_404_page_triggers_the_client_error_rule() {
    let mut p = page("https://e.com/gone");
    p.status = 404;
    let issues = run(&ClientError, &p);
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].rule_id, "response.4xx");
    assert_eq!(issues[0].detail.as_deref(), Some("HTTP 404"));
}

#[test]
fn a_200_page_does_not_trigger_the_client_error_rule() {
    assert!(run(&ClientError, &page("https://e.com/ok")).is_empty());
    // Nor does a 5xx: that is a different rule, and double-reporting one
    // problem as two would inflate every per-rule count.
    let mut p = page("https://e.com/boom");
    p.status = 503;
    assert!(run(&ClientError, &p).is_empty());
}

// ---- response.5xx -------------------------------------------------------

#[test]
fn a_503_page_triggers_the_server_error_rule() {
    let mut p = page("https://e.com/boom");
    p.status = 503;
    let issues = run(&ServerError, &p);
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].rule_id, "response.5xx");
}

#[test]
fn a_404_page_does_not_trigger_the_server_error_rule() {
    let mut p = page("https://e.com/gone");
    p.status = 404;
    assert!(run(&ServerError, &p).is_empty());
    assert!(run(&ServerError, &page("https://e.com/ok")).is_empty());
}

// ---- response.redirect-chain -------------------------------------------

#[test]
fn three_redirects_trigger_the_chain_rule() {
    let mut p = page("https://e.com/final");
    p.redirect_chain = vec![
        "https://e.com/a".into(),
        "https://e.com/b".into(),
        "https://e.com/c".into(),
    ];
    let issues = run(&LongRedirectChain, &p);
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].detail.as_deref(), Some("3 redirects"));
}

#[test]
fn two_redirects_do_not_trigger_the_chain_rule() {
    // The boundary is the whole rule: two is tolerated, three is not. An
    // off-by-one here flags every ordinary http-to-https-plus-slash redirect.
    let mut p = page("https://e.com/final");
    p.redirect_chain = vec!["https://e.com/a".into(), "https://e.com/b".into()];
    assert!(run(&LongRedirectChain, &p).is_empty());
    assert!(run(&LongRedirectChain, &page("https://e.com/direct")).is_empty());
}

// ---- response.mixed-content --------------------------------------------

#[test]
fn an_https_page_linking_to_http_triggers_mixed_content() {
    let mut p = page("https://e.com/secure");
    p.links = vec![link("http://insecure.example/x", "https://e.com/secure")];
    let issues = run(&MixedContent, &p);
    assert_eq!(issues.len(), 1);
    assert_eq!(
        issues[0].detail.as_deref(),
        Some("http://insecure.example/x")
    );
}

#[test]
fn neither_an_https_link_nor_an_http_page_triggers_mixed_content() {
    // An https page linking to https is fine...
    let mut secure = page("https://e.com/secure");
    secure.links = vec![link("https://other.example/x", "https://e.com/secure")];
    assert!(run(&MixedContent, &secure).is_empty());

    // ...and an http page linking to http is not "mixed" — it is uniformly
    // insecure, which is a different finding and not this rule's business.
    let mut plain = page("http://e.com/plain");
    plain.links = vec![link("http://other.example/x", "http://e.com/plain")];
    assert!(run(&MixedContent, &plain).is_empty());
}

// ---- response.redirect-loop --------------------------------------------

fn seed_redirect(store: &mut Store, source: &str, outcome: &str) {
    let mut writer = Writer::with_batch_size(store, 4);
    // crawl_redirects.source_url references frontier(url): a redirect source is
    // always a URL the crawl discovered first, so the fixture has to be too.
    writer
        .discover(&[(CrawlUrl::parse(source).unwrap(), 1)])
        .unwrap();
    let hops = vec![RedirectHop {
        url: CrawlUrl::parse(source).unwrap(),
        status: 302,
        location: "/next".into(),
        target: CrawlUrl::parse("https://e.com/next").ok(),
    }];
    writer
        .redirect(&CrawlUrl::parse(source).unwrap(), None, &hops, outcome)
        .unwrap();
    writer.flush().unwrap();
}

#[test]
fn a_looping_redirect_triggers_the_loop_rule() {
    let mut store = Store::in_memory().unwrap();
    seed_redirect(
        &mut store,
        "https://e.com/loop",
        "redirect loop at https://e.com/loop",
    );

    let found = RedirectLoop.check(&store).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, "https://e.com/loop");
    assert_eq!(found[0].1.rule_id, "response.redirect-loop");
    assert_eq!(found[0].1.detail.as_deref(), Some("1 hops"));
}

#[test]
fn a_redirect_that_landed_does_not_trigger_the_loop_rule() {
    let mut store = Store::in_memory().unwrap();
    seed_redirect(&mut store, "https://e.com/moved", "landed");
    assert!(RedirectLoop.check(&store).unwrap().is_empty());
}

// ---- the batch as a whole ----------------------------------------------

#[test]
fn the_batch_registers_five_rules_with_distinct_ids() {
    let mut reg = Registry::new();
    pounce_audit::rules::response::register(&mut reg).unwrap();
    assert_eq!(reg.len(), 5);
    assert_eq!(reg.page_rules().len(), 4);
    assert_eq!(
        reg.site_rules().len(),
        1,
        "the loop rule cannot be a page rule"
    );
}

#[test]
fn registering_the_batch_twice_is_rejected() {
    // Guards against a batch being added to register_all twice, which would
    // otherwise double every count in this batch.
    let mut reg = Registry::new();
    pounce_audit::rules::response::register(&mut reg).unwrap();
    assert!(pounce_audit::rules::response::register(&mut reg).is_err());
}
