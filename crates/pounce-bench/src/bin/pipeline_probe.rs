//! Child workload for the bounded-pipeline RSS measurement.

use pounce_core::{CrawlUrl, PipelineConfig, run_pipeline};
use std::time::Duration;

struct ProbePage {
    url: CrawlUrl,
    body: Box<[u8; 4096]>,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let pages = std::env::args()
        .nth(1)
        .expect("page count")
        .parse::<usize>()
        .expect("numeric page count");
    let mut checksum = 0u64;
    let mut written = 0usize;
    let stats = run_pipeline(
        (0..pages).map(|n| {
            CrawlUrl::parse(&format!("https://example.test/page/{n}"))
                .expect("generated URL is valid")
        }),
        PipelineConfig {
            channel_capacity: 32,
            fetch_concurrency: 16,
            parse_concurrency: 4,
        },
        |url| async move {
            let mut body = Box::new([0u8; 4096]);
            body[0] = url.as_url().path().len() as u8;
            ProbePage { url, body }
        },
        std::convert::identity,
        |page| {
            checksum = checksum
                .wrapping_add(u64::from(page.body[0]))
                .wrapping_add(page.url.as_url().path().len() as u64);
            written += 1;
            if written.is_multiple_of(1_000) {
                std::thread::sleep(Duration::from_millis(1));
            }
            Ok::<_, ()>(())
        },
    )
    .await
    .expect("pipeline should complete");
    println!("pages={} checksum={checksum}", stats.written);
}
