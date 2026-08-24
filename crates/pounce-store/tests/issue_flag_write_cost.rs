//! What maintaining `has_issue` costs the crawl's hot path.
//!
//! One `UPDATE pages SET has_issue = 1` per page that has findings, inside the
//! batch that just inserted the page. T3.0's rule applies: a write-path change
//! at scale is measured before it is believed, because this project's surprises
//! live there.
//!
//! Interleaved pairs with alternating order, medians, both arms asserted to the
//! same row counts.
//!
//! ```text
//! cargo test --release -p pounce-store --test issue_flag_write_cost -- --ignored --nocapture
//! ```

mod common;

use pounce_store::Store;
use std::time::{Duration, Instant};

/// Seeds `pages` pages, optionally writing the findings that flag them.
fn arm(dir: &std::path::Path, pages: u64, with_issues: bool) -> Duration {
    let path = dir.join(if with_issues {
        "with.pounce"
    } else {
        "without.pounce"
    });
    let _ = std::fs::remove_file(&path);
    let mut store = Store::open(&path).unwrap();
    let start = Instant::now();
    if with_issues {
        common::seed_pages_only(&mut store, pages);
    } else {
        common::seed_pages_without_issues(&mut store, pages);
    }
    let elapsed = start.elapsed();

    let rows: u64 = store
        .conn()
        .query_row("SELECT count(*) FROM pages", [], |r| r.get::<_, i64>(0))
        .unwrap() as u64;
    assert_eq!(rows, pages, "both arms must write the same pages");
    elapsed
}

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

#[test]
#[ignore = "seeds 100k pages ten times; run with --release --ignored --nocapture"]
fn the_flag_costs_the_write_path_this_much() {
    let pages: u64 = std::env::var("FLAG_COST_PAGES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000);
    let pairs: usize = std::env::var("FLAG_COST_PAIRS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);
    let dir = tempfile::tempdir().unwrap();

    let (mut with, mut without) = (Vec::new(), Vec::new());
    for pair in 0..pairs {
        // Alternating order, so warm-up or throttling cannot favour whichever
        // arm always runs first.
        for flag in if pair % 2 == 0 {
            [true, false]
        } else {
            [false, true]
        } {
            let took = arm(dir.path(), pages, flag);
            eprintln!(
                "pair {pair} {}: {took:?} ({:.0} pages/s)",
                if flag {
                    "issues + flag"
                } else {
                    "pages only  "
                },
                pages as f64 / took.as_secs_f64()
            );
            if flag {
                with.push(took)
            } else {
                without.push(took)
            }
        }
    }

    let (a, b) = (median(without), median(with));
    eprintln!("\n=== {pages} pages, medians of {pairs} ===");
    eprintln!("  pages only:            {a:?}");
    eprintln!("  issues + has_issue:    {b:?}");
    eprintln!(
        "  writing findings and flagging their pages costs {:+.2}%",
        (b.as_secs_f64() - a.as_secs_f64()) / a.as_secs_f64() * 100.0
    );
}
