//! The measured half of T1.4, against the real fixture server on the real
//! clock. The unit tests in `limit.rs` prove the schedule; these two prove it
//! survives contact with sockets.
//!
//! Ignored by default because the first takes 30 seconds. Run them with:
//!
//! ```bash
//! cargo test --release -p pounce-http --test politeness -- --ignored --nocapture
//! ```

use pounce_bench::graph::{GraphSpec, SiteGraph};
use pounce_bench::server::{Fixture, serve};
use pounce_http::limit::Limiter;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Far more askers than the limits allow, so the limiter shapes the traffic
/// rather than the workers running out of things to do.
const WORKERS: usize = 32;

struct Counts {
    /// Requests whose slot fell inside the measurement window.
    started: usize,
    peak_in_flight: usize,
}

async fn spawn_fixture() -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let fixture = Arc::new(Fixture {
        graph: SiteGraph::generate(&GraphSpec {
            seed: 5,
            page_count: 500,
            ..GraphSpec::default()
        }),
        base_url: base.clone(),
    });
    let handle = tokio::spawn(async move {
        let _ = serve(listener, fixture).await;
    });
    (base, handle)
}

/// Hammers the fixture through the limiter for `run`, and reports what the
/// limiter actually let through.
async fn drive(run: Duration, interval: Duration, cap: usize) -> Counts {
    let (base, server) = spawn_fixture().await;
    let limiter = Arc::new(Limiter::new(cap));
    let client = pounce_http::client().unwrap();

    let started = Arc::new(AtomicUsize::new(0));
    let in_flight = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));

    let clock = Instant::now();
    let mut workers = Vec::with_capacity(WORKERS);
    for w in 0..WORKERS {
        let (limiter, client, base) = (limiter.clone(), client.clone(), base.clone());
        let (started, in_flight, peak) = (started.clone(), in_flight.clone(), peak.clone());
        workers.push(tokio::spawn(async move {
            let mut n = w as u32;
            while clock.elapsed() < run {
                let _permit = limiter.acquire("127.0.0.1", interval).await;
                // The slot may have been reserved for after the window; those
                // requests are never sent and never counted.
                if clock.elapsed() >= run {
                    break;
                }
                started.fetch_add(1, Ordering::SeqCst);
                let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);

                let url = format!("{base}/page/{}", n % 500);
                let _ = client.get(&url).send().await.unwrap().bytes().await;

                in_flight.fetch_sub(1, Ordering::SeqCst);
                n += WORKERS as u32;
            }
        }));
    }
    for w in workers {
        let _ = w.await;
    }
    server.abort();

    Counts {
        started: started.load(Ordering::SeqCst),
        peak_in_flight: peak.load(Ordering::SeqCst),
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "takes 30 seconds; run with --ignored"]
async fn a_ten_per_second_cap_holds_over_a_thirty_second_run() {
    const RUN: Duration = Duration::from_secs(30);
    const RATE: u32 = 10;
    let budget = RATE as usize * RUN.as_secs() as usize;

    let c = drive(RUN, Duration::from_secs(1) / RATE, 8).await;
    let observed = c.started as f64 / RUN.as_secs_f64();
    println!(
        "{} requests in 30s = {observed:.2} req/s (cap {RATE})",
        c.started
    );

    assert!(
        c.started <= budget,
        "cap exceeded: {} > {budget}",
        c.started
    );
    // The fixture answers in well under 100ms, so falling short would mean the
    // limiter is over-delaying rather than pacing.
    assert!(
        c.started * 100 >= budget * 95,
        "far under the cap: {} of {budget}",
        c.started
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "hammers the fixture; run with --ignored"]
async fn the_concurrency_cap_binds_when_the_rate_limit_does_not() {
    // At 10 req/s against a fixture answering in under a millisecond, requests
    // never overlap and the concurrency cap is never reached — so it has to be
    // observed with the interval out of the way.
    const CAP: usize = 4;
    let c = drive(Duration::from_secs(2), Duration::ZERO, CAP).await;
    println!(
        "{} requests, peak in-flight {} (cap {CAP})",
        c.started, c.peak_in_flight
    );

    assert!(
        c.peak_in_flight <= CAP,
        "cap exceeded: {} in flight",
        c.peak_in_flight
    );
    assert_eq!(
        c.peak_in_flight, CAP,
        "the cap was never reached, so this run proves nothing"
    );
}
