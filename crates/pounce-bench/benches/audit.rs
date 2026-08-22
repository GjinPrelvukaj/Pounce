//! What audit rules cost on the crawl's hot path — the real shipped ruleset.
//!
//! Gate M2 allows rule execution 10% of crawl wall time. Records are built and
//! parsed *before* the timed section, so this measures the rule pass and
//! nothing else. An empty registry is the control: the difference between the
//! two bars is the entire budget question.
//!
//! The rules are the shipped set from `register_all` — not stand-ins. Every
//! page rule's real work is represented, including the ones that walk vectors
//! (`media.missing-alt` over images, `response.mixed-content` over links) and
//! the one that scans a string against an entity table
//! (`description.truncated-entity`). One deliberate construction: a quarter of
//! the corpus carries an https page URL while its links resolved to http, so
//! mixed-content runs its full per-link walk instead of short-circuiting on
//! the fixture's http scheme — the shape of a real site mid-migration.
//!
//! The eleven site rules run once per crawl against the finished database and
//! cannot be expressed per page; they are measured in
//! `pounce-audit/tests/site_rule_overhead.rs`, and the end-to-end A/B lives in
//! `pounce-cli`'s ignored tests. This bench is the per-page component only.

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use pounce_audit::Registry;
use pounce_bench::graph::{GraphSpec, SiteGraph};
use pounce_bench::render::render_page;
use pounce_core::CrawlUrl;
use pounce_parse::{BodyKind, MetaRobots, PageRecord, parse_body};
use std::hint::black_box;

const BASE: &str = "http://localhost:8080";
const PAGES: u32 = 5_000;

/// Fully-parsed records, built once outside the timed loop.
///
/// Every fourth record's URL becomes https *after* parsing: its links were
/// resolved against the http base and stay http, which is what makes
/// mixed-content walk all ~28 links instead of returning at the scheme check.
fn records(pages: u32) -> Vec<PageRecord> {
    let graph = SiteGraph::generate(&GraphSpec {
        seed: 42,
        page_count: pages,
        ..GraphSpec::default()
    });
    (0..pages)
        .map(|id| {
            let html = render_page(&graph, id, BASE);
            let path = &graph.nodes[id as usize].path;
            let url = CrawlUrl::parse(&format!("{BASE}{path}")).unwrap();
            let mut record = PageRecord {
                url,
                status: 200,
                depth: 2,
                size: html.len(),
                truncated: false,
                content_type: Some("text/html".into()),
                charset: Some("utf-8".into()),
                kind: BodyKind::Html,
                content_type_mismatch: false,
                elapsed_ms: 7,
                time_to_headers_ms: 3,
                redirect_chain: vec![],
                title: None,
                title_count: 0,
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
                word_count: 0,
                body_hash: None,
            };
            parse_body(&mut record, html.as_bytes()).unwrap();
            if id % 4 == 0 {
                record.url = CrawlUrl::parse(&format!("https://localhost:8080{path}")).unwrap();
            }
            record
        })
        .collect()
}

fn bench_rules(c: &mut Criterion) {
    let records = records(PAGES);

    let empty = Registry::new();
    let mut full = Registry::new();
    pounce_audit::register_all(&mut full).unwrap();
    assert_eq!(full.len(), 30, "the shipped set");
    assert_eq!(
        full.page_rules().len(),
        19,
        "eleven of the thirty are site rules, measured elsewhere"
    );

    // The corpus must actually exercise the rules: if the pass found nothing
    // anywhere it would be timing an empty branch, not a ruleset.
    let fired: usize = records.iter().map(|r| full.run_page(r).len()).sum();
    let mixed: usize = records
        .iter()
        .filter(|r| r.url.scheme() == "https")
        .map(|r| {
            full.run_page(r)
                .iter()
                .filter(|i| i.rule_id == "response.mixed-content")
                .count()
        })
        .sum();
    assert!(fired > 0 && mixed > 0, "the corpus must trigger findings");

    let mut group = c.benchmark_group("audit");
    group.throughput(Throughput::Elements(records.len() as u64));
    group.bench_function("no_rules", |b| {
        b.iter(|| {
            let mut found = 0usize;
            for r in &records {
                found += black_box(empty.run_page(black_box(r))).len();
            }
            black_box(found)
        })
    });
    group.bench_function("shipped_page_rules", |b| {
        b.iter(|| {
            let mut found = 0usize;
            for r in &records {
                found += black_box(full.run_page(black_box(r))).len();
            }
            black_box(found)
        })
    });
    group.finish();

    // One bar per rule, so an expensive rule names itself instead of hiding
    // inside an average. All fixture records are HTML, so `applies()` is true
    // throughout and calling `check` directly measures the same work.
    let mut alone = c.benchmark_group("rule_alone");
    alone.throughput(Throughput::Elements(records.len() as u64));
    for rule in full.page_rules() {
        let id = rule.meta().id;
        alone.bench_function(id, |b| {
            b.iter(|| {
                let mut out = Vec::new();
                for r in &records {
                    rule.check(black_box(r), &mut out);
                }
                black_box(out.len())
            })
        });
    }
    alone.finish();
}

criterion_group!(benches, bench_rules);
criterion_main!(benches);
