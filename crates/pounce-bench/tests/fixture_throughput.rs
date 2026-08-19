//! Measures the fixture server's own serving ceiling.
//!
//! This is a control measurement, and it is the most important one in the
//! harness. If the fixture cannot serve faster than the crawlers under test,
//! every benchmark measures the fixture and the comparison is meaningless.
//!
//! Ignored by default — it is a load test, not a correctness test.
//! Run explicitly, in release:
//!
//! ```bash
//! cargo test --release -p pounce-bench --test fixture_throughput -- --ignored --nocapture
//! ```

use pounce_bench::graph::{GraphSpec, SiteGraph};
use pounce_bench::server::{Fixture, serve};
use std::sync::Arc;
use std::time::Instant;

async fn spawn(pages: u32) -> String {
    let graph = SiteGraph::generate(&GraphSpec {
        seed: 42,
        page_count: pages,
        ..GraphSpec::default()
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let fixture = Arc::new(Fixture {
        graph,
        base_url: base.clone(),
    });
    tokio::spawn(async move { serve(listener, fixture).await });
    base
}

/// Fires `total` requests at `concurrency`, reusing one connection pool.
async fn hammer(base: &str, paths: &[String], concurrency: usize) -> (f64, u64) {
    let client = Arc::new(
        reqwest::Client::builder()
            .pool_max_idle_per_host(concurrency)
            .build()
            .unwrap(),
    );
    let start = Instant::now();
    let mut bytes = 0u64;

    for chunk in paths.chunks(concurrency) {
        let mut set = Vec::with_capacity(chunk.len());
        for p in chunk {
            let c = client.clone();
            let url = format!("{base}{p}");
            set.push(tokio::spawn(async move {
                let r = c.get(&url).send().await.unwrap();
                assert_eq!(r.status(), 200);
                r.bytes().await.unwrap().len() as u64
            }));
        }
        for h in set {
            bytes += h.await.unwrap();
        }
    }

    let secs = start.elapsed().as_secs_f64();
    (paths.len() as f64 / secs, bytes)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "load test; run explicitly in release"]
async fn reports_the_fixture_serving_ceiling() {
    const PAGES: u32 = 5_000;
    const REQUESTS: usize = 5_000;

    let base = spawn(PAGES).await;
    let graph = SiteGraph::generate(&GraphSpec {
        seed: 42,
        page_count: PAGES,
        ..GraphSpec::default()
    });
    let paths: Vec<String> = (0..REQUESTS)
        .map(|i| graph.node((i as u32) % PAGES).path.clone())
        .collect();

    // Warm the pool and the allocator before measuring.
    let _ = hammer(&base, &paths[..200], 50).await;

    for concurrency in [1usize, 16, 64, 200] {
        let (rps, bytes) = hammer(&base, &paths, concurrency).await;
        let mib_s = bytes as f64 / (1024.0 * 1024.0) / (REQUESTS as f64 / rps);
        println!("concurrency {concurrency:>3}: {rps:>9.0} req/s   {mib_s:>7.1} MiB/s");
    }

    println!(
        "\nIf these numbers are not far above any crawler under test,\n\
         the benchmark is measuring the fixture, not the crawler."
    );
}
