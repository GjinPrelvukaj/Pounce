//! Release-only proof that bounded stage handoffs keep memory independent of
//! crawl size.

use pounce_bench::metrics::{Measurement, run_measured};
use std::time::Duration;

fn measure(pages: usize) -> Measurement {
    let command = vec![
        env!("CARGO_BIN_EXE_pipeline-probe").to_string(),
        pages.to_string(),
    ];
    run_measured(&command, Some(Duration::from_secs(60))).unwrap()
}

#[test]
#[ignore = "load test; run explicitly in release"]
fn peak_rss_stays_flat_from_50k_to_500k_urls() {
    let small = measure(50_000);
    let large = measure(500_000);
    assert_eq!(small.exit_code, 0);
    assert_eq!(large.exit_code, 0);
    assert!(!small.timed_out && !large.timed_out);

    let delta = large.peak_rss_bytes.saturating_sub(small.peak_rss_bytes);
    println!(
        "50k peak: {:.2} MiB; 500k peak: {:.2} MiB; delta: {:.2} MiB",
        small.peak_rss_bytes as f64 / 1_048_576.0,
        large.peak_rss_bytes as f64 / 1_048_576.0,
        delta as f64 / 1_048_576.0,
    );
    assert!(
        delta <= 8 * 1_048_576,
        "RSS grew by {:.2} MiB",
        delta as f64 / 1_048_576.0
    );
}
