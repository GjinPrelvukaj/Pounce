//! What audit rules cost on the crawl's hot path.
//!
//! Gate M2 allows rule execution 10% of crawl wall time. Records are built and
//! parsed *before* the timed section, so this measures the rule pass and
//! nothing else. An empty registry is the control: the difference between the
//! two bars is the entire budget question.
//!
//! The rules here are stand-ins with the shape of real ones — a field read and
//! a comparison — not the real thirty. What this bench establishes is the cost
//! of the machinery and of thirty cheap checks; a rule that does something
//! expensive is not represented, and the honest figure only arrives when the
//! batches land.

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use pounce_audit::{Issue, PageRule, Registry, RuleMeta, Severity};
use pounce_bench::graph::{GraphSpec, SiteGraph};
use pounce_bench::render::render_page;
use pounce_core::CrawlUrl;
use pounce_parse::{BodyKind, MetaRobots, PageRecord, parse_body};
use std::hint::black_box;

const BASE: &str = "http://localhost:8080";
const PAGES: u32 = 5_000;

/// A stand-in with the shape of a real page rule: read a field, compare, maybe
/// push. Half of them fire, so the `Vec` growth is represented too.
struct Cheap(&'static str, usize);

impl PageRule for Cheap {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: self.0,
            severity: Severity::Warning,
            description: "bench",
            remediation: "bench",
        }
    }
    fn check(&self, page: &PageRecord, out: &mut Vec<Issue>) {
        let over = page.title.as_deref().is_none_or(|t| t.len() > self.1);
        if over {
            out.push(Issue {
                rule_id: self.0,
                severity: Severity::Warning,
                detail: None,
            });
        }
    }
}

/// Fully-parsed records, built once outside the timed loop.
fn records(pages: u32) -> Vec<PageRecord> {
    let graph = SiteGraph::generate(&GraphSpec {
        seed: 42,
        page_count: pages,
        ..GraphSpec::default()
    });
    (0..pages)
        .map(|id| {
            let html = render_page(&graph, id, BASE);
            let url = CrawlUrl::parse(&format!("{BASE}{}", graph.nodes[id as usize].path)).unwrap();
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
            record
        })
        .collect()
}

fn bench_rules(c: &mut Criterion) {
    let records = records(PAGES);

    let empty = Registry::new();
    let mut full = Registry::new();
    for i in 0..30 {
        let id: &'static str = Box::leak(format!("bench.rule-{i}").into_boxed_str());
        // Alternating thresholds so roughly half the rules fire per page.
        full.register_page(Box::new(Cheap(id, if i % 2 == 0 { 10 } else { 4096 })))
            .unwrap();
    }
    assert_eq!(full.len(), 30, "the bench must measure a full ruleset");

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
    group.bench_function("thirty_rules", |b| {
        b.iter(|| {
            let mut found = 0usize;
            for r in &records {
                found += black_box(full.run_page(black_box(r))).len();
            }
            black_box(found)
        })
    });
    group.finish();
}

criterion_group!(benches, bench_rules);
criterion_main!(benches);
