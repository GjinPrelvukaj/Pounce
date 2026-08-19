//! Runs one or more crawlers against an in-process fixture site and reports
//! throughput and peak memory for each.
//!
//! Every tool gets the same site, the same machine, and the same measurement
//! code. That is the whole point — a comparison where each tool reports its
//! own numbers is not a comparison.
//!
//! Usage:
//! ```text
//! bench-runner --pages 100000 \
//!   --tool 'pounce=./target/release/pounce crawl {url} --quiet' \
//!   --tool 'freecrawl=freecrawl crawl {url}' \
//!   --out bench-results/run.json
//! ```

use anyhow::{Context, Result, bail};
use clap::Parser;
use pounce_bench::graph::{GraphSpec, SiteGraph};
use pounce_bench::metrics::run_measured;
use pounce_bench::report::{BenchResult, Report};
use pounce_bench::server::{Fixture, serve};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(
    name = "bench-runner",
    about = "Benchmark crawlers against a deterministic fixture site"
)]
struct Args {
    #[arg(long, default_value_t = 100_000)]
    pages: u32,
    #[arg(long, default_value_t = 42)]
    seed: u64,
    /// A tool to benchmark, as `name=command`. `{url}` is replaced with the
    /// fixture site's base URL. Repeatable.
    #[arg(long = "tool", value_name = "NAME=COMMAND")]
    tools: Vec<String>,
    /// Per-tool timeout in seconds.
    #[arg(long, default_value_t = 1800)]
    timeout_secs: u64,
    /// Where to write the JSON report.
    #[arg(long, default_value = "bench-results/run.json")]
    out: PathBuf,
}

fn parse_tool(spec: &str) -> Result<(String, String)> {
    let Some((name, command)) = spec.split_once('=') else {
        bail!("tool spec must be NAME=COMMAND, got: {spec}");
    };
    if name.is_empty() || command.trim().is_empty() {
        bail!("tool spec has an empty name or command: {spec}");
    }
    Ok((name.to_string(), command.to_string()))
}

/// Splits a command string on whitespace. Deliberately simple: quoted
/// arguments containing spaces are not supported, and a tool needing them
/// should be wrapped in a shell script.
fn split_command(command: &str, url: &str) -> Vec<String> {
    command
        .split_whitespace()
        .map(|part| part.replace("{url}", url))
        .collect()
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if args.tools.is_empty() {
        bail!("at least one --tool is required");
    }
    let tools: Vec<(String, String)> = args
        .tools
        .iter()
        .map(|s| parse_tool(s))
        .collect::<Result<_>>()?;

    // Stand up the fixture site on an ephemeral port.
    let graph = SiteGraph::generate(&GraphSpec {
        seed: args.seed,
        page_count: args.pages,
        ..GraphSpec::default()
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let base_url = format!("http://{}", listener.local_addr()?);
    eprintln!(
        "fixture site: {base_url} ({} pages, seed {})",
        args.pages, args.seed
    );

    let fixture = Arc::new(Fixture {
        graph,
        base_url: base_url.clone(),
    });
    let server = tokio::spawn(async move { serve(listener, fixture).await });

    let mut results = Vec::new();
    for (name, command) in &tools {
        let argv = split_command(command, &base_url);
        eprintln!("running {name}: {}", argv.join(" "));

        // Measurement blocks; keep it off the async runtime so the fixture
        // server stays responsive while the tool crawls it.
        let argv2 = argv.clone();
        let timeout = Duration::from_secs(args.timeout_secs);
        let measurement =
            tokio::task::spawn_blocking(move || run_measured(&argv2, Some(timeout))).await??;

        eprintln!(
            "  {name}: {:.1}s, peak {:.0} MB, exit {}{}",
            measurement.wall_ms as f64 / 1000.0,
            measurement.peak_rss_bytes as f64 / (1024.0 * 1024.0),
            measurement.exit_code,
            if measurement.timed_out {
                " (TIMED OUT)"
            } else {
                ""
            }
        );

        results.push(BenchResult {
            tool: name.clone(),
            wall_ms: measurement.wall_ms,
            peak_rss_bytes: measurement.peak_rss_bytes,
            // Assumes the tool crawled the whole site. Once pounce-cli emits a
            // JSON summary, read the real count from it instead — a tool that
            // silently crawled half the site would otherwise look twice as fast.
            pages_crawled: u64::from(args.pages),
            exit_code: measurement.exit_code,
            timed_out: measurement.timed_out,
        });
    }

    server.abort();

    let report = Report {
        fixture_seed: args.seed,
        fixture_pages: args.pages,
        results,
    };

    if let Some(dir) = args.out.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("failed to create {}", dir.display()))?;
    }
    std::fs::write(&args.out, serde_json::to_string_pretty(&report)?)
        .with_context(|| format!("failed to write {}", args.out.display()))?;

    println!("\n{}", report.to_markdown());
    eprintln!("wrote {}", args.out.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_well_formed_tool_spec() {
        let (n, c) = parse_tool("pounce=pounce crawl {url}").unwrap();
        assert_eq!(n, "pounce");
        assert_eq!(c, "pounce crawl {url}");
    }

    #[test]
    fn keeps_equals_signs_in_the_command() {
        let (n, c) = parse_tool("t=cmd --flag=value {url}").unwrap();
        assert_eq!(n, "t");
        assert_eq!(c, "cmd --flag=value {url}");
    }

    #[test]
    fn rejects_malformed_tool_specs() {
        assert!(parse_tool("no-equals-sign").is_err());
        assert!(parse_tool("=empty-name").is_err());
        assert!(parse_tool("empty-command=   ").is_err());
    }

    #[test]
    fn substitutes_the_url_placeholder() {
        let argv = split_command("crawler --seed {url} --quiet", "http://127.0.0.1:9999");
        assert_eq!(
            argv,
            vec!["crawler", "--seed", "http://127.0.0.1:9999", "--quiet"]
        );
    }

    #[test]
    fn substitutes_every_occurrence_of_the_placeholder() {
        let argv = split_command("c {url} --base {url}", "http://x");
        assert_eq!(argv, vec!["c", "http://x", "--base", "http://x"]);
    }
}
