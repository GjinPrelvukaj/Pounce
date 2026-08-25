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

/// Seeds `pages` pages, optionally writing the findings that flag them, and
/// returns the write time and the end-of-crawl index build separately.
///
/// The second number is the one the 500k A/B went looking for: the issue
/// indices and the `has_issue` rebuild are deferred to `build_query_indices`,
/// so an arm with no findings builds them over an empty table for free. That
/// cost exists only because the rules ran, and it lands after the last page.
fn arm(dir: &std::path::Path, pages: u64, with_issues: bool) -> (Duration, Duration) {
    let path = dir.join(if with_issues {
        "with.pounce"
    } else {
        "without.pounce"
    });
    let _ = std::fs::remove_file(&path);
    let mut store = Store::open(&path).unwrap();
    // `FLAG_COST_DENSITY` writes exactly that many findings on every page,
    // instead of `seed_pages_only`'s ~0.67. A real crawl of the bench fixture
    // produces 2.24, and whether the cost per issue is linear in density is the
    // open question in the 500k rule-overhead measurement.
    let density: Option<usize> = std::env::var("FLAG_COST_DENSITY")
        .ok()
        .and_then(|v| v.parse().ok());
    // `seed_pages_only` batches 5,000 rows; the crawl's `Writer` batches 500.
    // Ten times the commits is ten times the transaction overhead, and a
    // store-side instrument that does not match it under-prices the write path
    // it is standing in for.
    let batch: usize = std::env::var("FLAG_COST_BATCH")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5_000);
    // The bench fixture emits 28 links per page and this seeder emitted none.
    let links: usize = std::env::var("FLAG_COST_LINKS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let start = Instant::now();
    match (with_issues, density) {
        (true, Some(per_page)) => {
            common::seed_pages_with_issue_density(&mut store, pages, per_page, batch, links)
        }
        (true, None) => common::seed_pages_only(&mut store, pages),
        // The no-findings arm has to use the same batch size, or the comparison
        // prices the batching rather than the findings.
        (false, Some(_)) => {
            common::seed_pages_with_issue_density(&mut store, pages, 0, batch, links)
        }
        (false, None) => common::seed_pages_without_issues(&mut store, pages),
    }
    let elapsed = start.elapsed();

    let indexing = Instant::now();
    store.build_query_indices().unwrap();
    let indexing = indexing.elapsed();

    let rows: u64 = store
        .conn()
        .query_row("SELECT count(*) FROM pages", [], |r| r.get::<_, i64>(0))
        .unwrap() as u64;
    assert_eq!(rows, pages, "both arms must write the same pages");
    (elapsed, indexing)
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
    let (mut with_idx, mut without_idx) = (Vec::new(), Vec::new());
    for pair in 0..pairs {
        // Alternating order, so warm-up or throttling cannot favour whichever
        // arm always runs first.
        for flag in if pair % 2 == 0 {
            [true, false]
        } else {
            [false, true]
        } {
            let (took, indexing) = arm(dir.path(), pages, flag);
            eprintln!(
                "pair {pair} {}: {took:?} write, {indexing:?} index ({:.0} pages/s)",
                if flag {
                    "issues + flag"
                } else {
                    "pages only  "
                },
                pages as f64 / took.as_secs_f64()
            );
            if flag {
                with.push(took);
                with_idx.push(indexing);
            } else {
                without.push(took);
                without_idx.push(indexing);
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

    let (c, d) = (median(without_idx), median(with_idx));
    eprintln!("  build_query_indices, no findings:   {c:?}");
    eprintln!("  build_query_indices, with findings: {d:?}");
    eprintln!(
        "  indexing the findings adds {:?} — {:+.2}% of the write path",
        d.saturating_sub(c),
        (d.as_secs_f64() - c.as_secs_f64()) / a.as_secs_f64() * 100.0
    );
}
