//! Parse-throughput benchmarks.
//!
//! Measures link and metadata extraction over fixture pages, which is the hot
//! path in any crawl and the single biggest claimed advantage over the Node
//! and Python competitors. Throughput is reported in bytes/sec so results stay
//! comparable as fixture page sizes change.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use lol_html::{HtmlRewriter, Settings, element};
use pounce_bench::graph::{GraphSpec, SiteGraph};
use pounce_bench::render::render_page;
use std::cell::Cell;
use std::hint::black_box;

const BASE: &str = "http://localhost:8080";

fn corpus(pages: u32) -> Vec<String> {
    let graph = SiteGraph::generate(&GraphSpec {
        seed: 42,
        page_count: pages,
        ..GraphSpec::default()
    });
    (0..pages).map(|id| render_page(&graph, id, BASE)).collect()
}

/// Extracts what a crawler actually needs — links, canonical, description,
/// and images without alt — in a single streaming pass, building no DOM.
fn extract(html: &str) -> usize {
    // Cell rather than a plain counter: each handler is a separate closure,
    // and four of them cannot hold mutable borrows of the same local.
    let found = Cell::new(0usize);
    let bump = |c: &Cell<usize>| c.set(c.get() + 1);

    // lol_html 3.0 made Settings' fields private; it is a builder now.
    let settings = Settings::new()
        .append_element_content_handler(element!("a[href]", |el| {
            if el.get_attribute("href").is_some() {
                bump(&found);
            }
            Ok(())
        }))
        .append_element_content_handler(element!("link[rel=canonical]", |el| {
            if el.get_attribute("href").is_some() {
                bump(&found);
            }
            Ok(())
        }))
        .append_element_content_handler(element!("meta[name=description]", |el| {
            if el.get_attribute("content").is_some() {
                bump(&found);
            }
            Ok(())
        }))
        .append_element_content_handler(element!("img", |el| {
            if el.get_attribute("alt").is_none() {
                bump(&found);
            }
            Ok(())
        }));

    let mut rewriter = HtmlRewriter::new(settings, |_: &[u8]| {});
    rewriter.write(html.as_bytes()).unwrap();
    rewriter.end().unwrap();
    found.get()
}

fn bench_extract(c: &mut Criterion) {
    let pages = corpus(200);
    let total_bytes: usize = pages.iter().map(|p| p.len()).sum();
    let mean_kb = total_bytes as f64 / pages.len() as f64 / 1024.0;

    let mut group = c.benchmark_group("extract");
    group.throughput(Throughput::Bytes(total_bytes as u64));
    group.bench_function(
        BenchmarkId::new("lol_html", format!("{mean_kb:.0}KB_pages")),
        |b| {
            b.iter(|| {
                let mut total = 0usize;
                for page in &pages {
                    total += extract(black_box(page));
                }
                black_box(total)
            })
        },
    );
    group.finish();
}

fn bench_render(c: &mut Criterion) {
    // Fixture generation speed matters too: a slow generator makes the whole
    // harness annoying to run, and annoying harnesses stop being run.
    let graph = SiteGraph::generate(&GraphSpec {
        seed: 42,
        page_count: 1000,
        ..GraphSpec::default()
    });
    c.bench_function("render_page", |b| {
        b.iter(|| black_box(render_page(&graph, black_box(500), BASE)))
    });
}

criterion_group!(benches, bench_extract, bench_render);
criterion_main!(benches);
