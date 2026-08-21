//! Insert throughput for the batched writer.
//!
//! Measures the writer alone: the records are built and extracted *before* the
//! timed section, so what is reported is transaction and index cost, not parse
//! cost. Runs against a real file rather than `:memory:` — an in-memory number
//! would be flattering and would say nothing about the thing that actually
//! throttles a crawl.
//!
//! The batch size is the parameter under test. The spec asserts ~500; this is
//! how we find out whether that was a good guess.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use pounce_bench::graph::{GraphSpec, SiteGraph};
use pounce_bench::render::render_page;
use pounce_core::CrawlUrl;
use pounce_parse::{PageRecord, parse_body};
use pounce_store::{CrawlState, Store, Writer};
use std::hint::black_box;

const BASE: &str = "http://localhost:8080";
const PAGES: u32 = 5_000;

/// Fully-populated records, built once outside the timed loop.
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
            let mut record = seed(url, html.len());
            parse_body(&mut record, html.as_bytes()).unwrap();
            record
        })
        .collect()
}

fn seed(url: CrawlUrl, size: usize) -> PageRecord {
    PageRecord {
        url,
        status: 200,
        depth: 2,
        size,
        truncated: false,
        content_type: Some("text/html".into()),
        charset: Some("utf-8".into()),
        kind: pounce_parse::BodyKind::Undeclared,
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
        meta_robots: pounce_parse::MetaRobots::default(),
        hreflang: vec![],
        open_graph: vec![],
        links: vec![],
        images: vec![],
        word_count: 0,
        body_hash: None,
    }
}

fn bench_insert(c: &mut Criterion) {
    let records = records(PAGES);
    let frontier = records
        .iter()
        .map(|record| (record.url.clone(), record.depth))
        .collect::<Vec<_>>();

    let mut group = c.benchmark_group("store_insert");
    group.throughput(Throughput::Elements(records.len() as u64));
    group.sample_size(10);

    for batch in [1usize, 100, 500, 2000] {
        group.bench_with_input(BenchmarkId::new("batch", batch), &batch, |b, &batch| {
            b.iter_batched(
                // A fresh database per iteration; otherwise the second
                // iteration measures upserts into a warm index instead of
                // inserts into an empty one.
                || {
                    let dir = tempfile::tempdir().unwrap();
                    let mut store = Store::open(dir.path().join("bench.pounce")).unwrap();
                    {
                        let mut state = CrawlState::new(&mut store);
                        state.start(&records[0].url).unwrap();
                        state.discover(&frontier[1..]).unwrap();
                    }
                    (dir, store)
                },
                |(dir, mut store)| {
                    {
                        let mut writer = Writer::with_batch_size(&mut store, batch);
                        for record in &records {
                            writer.push(black_box(record)).unwrap();
                        }
                        writer.flush().unwrap();
                    }
                    // Returned so the drop cost lands outside the closure
                    // body but inside the measurement, as it would in a
                    // real crawl.
                    (dir, store)
                },
                criterion::BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

criterion_group!(benches, bench_insert);
criterion_main!(benches);
