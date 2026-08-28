//! Standalone fixture site server.
//!
//! Usage: `fixture-site --pages 100000 --seed 42 --port 8080`
//!
//! stdout carries only the base URL, so a runner can capture it cleanly;
//! everything else goes to stderr.

use clap::Parser;
use pounce_bench::graph::{GraphSpec, SiteGraph};
use pounce_bench::server::{Fixture, serve};
use std::sync::Arc;
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(
    name = "fixture-site",
    about = "Deterministic fixture website for benchmarking"
)]
struct Args {
    #[arg(long, default_value_t = 100_000)]
    pages: u32,
    #[arg(long, default_value_t = 42)]
    seed: u64,
    #[arg(long, default_value_t = 8080)]
    port: u16,
    #[arg(long, default_value_t = 6)]
    max_depth: u16,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let start = Instant::now();
    let graph = SiteGraph::generate(&GraphSpec {
        seed: args.seed,
        page_count: args.pages,
        max_depth: args.max_depth,
        ..GraphSpec::default()
    });
    let total_links: usize = graph.nodes.iter().map(|n| n.outlinks.len()).sum();
    eprintln!(
        "generated {} pages, {} links, mean {:.1} links/page in {:?}",
        graph.nodes.len(),
        total_links,
        total_links as f64 / graph.nodes.len() as f64,
        start.elapsed()
    );

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", args.port)).await?;
    let addr = listener.local_addr()?;
    let base_url = format!("http://{addr}");
    eprintln!("fixture site ready at {base_url} (seed {})", args.seed);
    println!("{base_url}");

    serve(listener, Arc::new(Fixture::new(graph, base_url))).await
}
