//! Benchmark results and their published representations.
//!
//! The markdown table produced here is the project's primary marketing asset,
//! so it has to be honest: timed-out and errored runs are labelled in the
//! table rather than dropped from it. A benchmark that quietly omits the cases
//! where a competitor struggles is an advertisement, and it will be found out.

use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchResult {
    pub tool: String,
    pub wall_ms: u128,
    pub peak_rss_bytes: u64,
    pub pages_crawled: u64,
    pub exit_code: i32,
    pub timed_out: bool,
}

impl BenchResult {
    pub fn urls_per_sec(&self) -> f64 {
        if self.wall_ms == 0 {
            return 0.0;
        }
        self.pages_crawled as f64 / (self.wall_ms as f64 / 1000.0)
    }

    pub fn peak_rss_mb(&self) -> f64 {
        self.peak_rss_bytes as f64 / (1024.0 * 1024.0)
    }

    /// Human-readable outcome. Anything other than "ok" means the throughput
    /// figure beside it is not a like-for-like result.
    pub fn status(&self) -> String {
        if self.timed_out {
            "timed out".to_string()
        } else if self.exit_code != 0 {
            format!("exit {}", self.exit_code)
        } else {
            "ok".to_string()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub fixture_seed: u64,
    pub fixture_pages: u32,
    pub results: Vec<BenchResult>,
}

impl Report {
    pub fn to_markdown(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(
            s,
            "Fixture: {} pages, seed {}.\n",
            self.fixture_pages, self.fixture_seed
        );
        s.push_str("| Tool | URLs/sec | Peak RSS (MB) | Wall (s) | Pages | Status |\n");
        s.push_str("|---|---:|---:|---:|---:|---|\n");
        for r in &self.results {
            let _ = writeln!(
                s,
                "| {} | {:.0} | {:.0} | {:.1} | {} | {} |",
                r.tool,
                r.urls_per_sec(),
                r.peak_rss_mb(),
                r.wall_ms as f64 / 1000.0,
                r.pages_crawled,
                r.status()
            );
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Report {
        Report {
            fixture_seed: 42,
            fixture_pages: 100_000,
            results: vec![
                BenchResult {
                    tool: "pounce".into(),
                    wall_ms: 40_000,
                    peak_rss_bytes: 300 * 1024 * 1024,
                    pages_crawled: 100_000,
                    exit_code: 0,
                    timed_out: false,
                },
                BenchResult {
                    tool: "freecrawl".into(),
                    wall_ms: 120_000,
                    peak_rss_bytes: 1500 * 1024 * 1024,
                    pages_crawled: 100_000,
                    exit_code: 0,
                    timed_out: false,
                },
            ],
        }
    }

    #[test]
    fn computes_throughput() {
        assert_eq!(sample().results[0].urls_per_sec(), 2500.0);
    }

    #[test]
    fn throughput_is_zero_when_no_time_elapsed() {
        let r = BenchResult {
            wall_ms: 0,
            ..sample().results[0].clone()
        };
        assert_eq!(r.urls_per_sec(), 0.0);
    }

    #[test]
    fn converts_bytes_to_megabytes() {
        assert_eq!(sample().results[0].peak_rss_mb(), 300.0);
    }

    #[test]
    fn renders_a_markdown_table_with_a_row_per_tool() {
        let md = sample().to_markdown();
        assert!(md.contains("| Tool |"));
        assert!(md.contains("| pounce |"));
        assert!(md.contains("| freecrawl |"));
        assert!(md.contains("2500"));
        assert!(md.contains("seed 42"));
    }

    #[test]
    fn marks_timed_out_runs_in_the_table() {
        let mut r = sample();
        r.results[1].timed_out = true;
        assert!(r.to_markdown().contains("timed out"));
    }

    #[test]
    fn marks_failed_runs_in_the_table() {
        let mut r = sample();
        r.results[1].exit_code = 2;
        assert!(r.to_markdown().contains("exit 2"));
    }

    #[test]
    fn a_timeout_outranks_an_exit_code_in_the_status() {
        let r = BenchResult {
            timed_out: true,
            exit_code: 1,
            ..sample().results[0].clone()
        };
        assert_eq!(r.status(), "timed out");
    }

    #[test]
    fn serialises_to_json_round_trip() {
        let json = serde_json::to_string(&sample()).unwrap();
        let back: Report = serde_json::from_str(&json).unwrap();
        assert_eq!(back.results.len(), 2);
        assert_eq!(back.fixture_seed, 42);
        assert_eq!(back.results[0].tool, "pounce");
    }
}
