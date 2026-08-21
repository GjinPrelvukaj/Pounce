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
    /// How to count what a tool actually crawled, as `NAME=COMMAND`.
    ///
    /// Run after that tool finishes; its stdout must be a single integer. The
    /// runner cannot know each tool's output format, so the operator supplies
    /// the one-liner — `sqlite3 out.pounce 'select count(*) from pages'`, or a
    /// `jq` over a competitor's JSON summary. Without one the tool's crawled
    /// count and throughput are reported as unknown rather than guessed.
    #[arg(long = "count", value_name = "NAME=COMMAND")]
    counts: Vec<String>,
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

/// Runs a tool's count command and reads a single integer from its stdout.
///
/// Any failure — spawn, non-zero exit, unparseable output — yields `None` and a
/// warning. A count that cannot be trusted must not become a throughput figure,
/// and failing the whole benchmark over it would throw away a good measurement.
fn measure_count(command: &str, url: &str) -> Option<u64> {
    let argv = split_command(command, url);
    let (program, rest) = argv.split_first()?;
    let output = match std::process::Command::new(program).args(rest).output() {
        Ok(output) => output,
        Err(e) => {
            eprintln!("  count command failed to run ({e}); reporting unknown");
            return None;
        }
    };
    if !output.status.success() {
        eprintln!(
            "  count command exited {}; reporting unknown",
            output.status.code().unwrap_or(-1)
        );
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    match text.trim().parse::<u64>() {
        Ok(n) => Some(n),
        Err(_) => {
            eprintln!("  count command printed {text:?}, not an integer; reporting unknown");
            None
        }
    }
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
    let counts: std::collections::HashMap<String, String> = args
        .counts
        .iter()
        .map(|s| parse_tool(s))
        .collect::<Result<_>>()?;
    // A --count naming no --tool is a typo that would otherwise silently
    // produce an unknown count in the published table.
    for name in counts.keys() {
        if !tools.iter().any(|(tool, _)| tool == name) {
            bail!("--count names `{name}`, which is not one of the --tool entries");
        }
    }

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

        // Measured, never assumed. This used to default to `--pages`, which
        // published a throughput number for every tool derived from the belief
        // that it crawled everything it was pointed at.
        let pages_crawled = counts
            .get(name)
            .and_then(|command| measure_count(command, &base_url));
        if let Some(pages) = pages_crawled {
            eprintln!("  {name}: crawled {pages} URLs");
        }

        results.push(BenchResult {
            tool: name.clone(),
            wall_ms: measurement.wall_ms,
            peak_rss_bytes: measurement.peak_rss_bytes,
            pages_crawled,
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
    fn a_count_command_yields_the_integer_it_prints() {
        let got = measure_count("echo 10001", "http://x");
        assert_eq!(got, Some(10_001));
    }

    #[test]
    fn a_count_command_that_prints_nonsense_is_unknown_not_zero() {
        // Zero would be a measurement. Unknown is the truth.
        assert_eq!(measure_count("echo not-a-number", "http://x"), None);
    }

    #[test]
    fn a_count_command_that_cannot_run_is_unknown() {
        assert_eq!(
            measure_count("definitely-not-a-real-binary-xyz", "http://x"),
            None
        );
    }

    #[test]
    fn a_count_command_that_fails_is_unknown() {
        assert_eq!(measure_count("false", "http://x"), None);
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
