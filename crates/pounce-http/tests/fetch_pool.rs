//! The fetch pool against real servers: the fixture site for ordinary and
//! pathological responses, throwaway servers where a test needs to script the
//! failures or count the requests that arrived.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use pounce_bench::graph::{GraphSpec, SiteGraph};
use pounce_bench::server::{Fixture, serve};
use pounce_core::CrawlUrl;
use pounce_http::fetch::{FetchConfig, FetchError, Fetcher};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task::JoinHandle;

fn fetcher(config: FetchConfig) -> Fetcher {
    Fetcher::new(config).unwrap()
}

fn url(base: &str, path: &str) -> CrawlUrl {
    CrawlUrl::parse(&format!("{base}{path}")).unwrap()
}

/// Also returns the generated page paths. They look like `/guides/latency-7`,
/// not `/page/7` — inventing one is a silent 404.
async fn spawn_fixture() -> (String, Vec<String>, JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let graph = SiteGraph::generate(&GraphSpec {
        seed: 5,
        page_count: 50,
        ..GraphSpec::default()
    });
    let paths: Vec<String> = graph.nodes.iter().map(|n| n.path.clone()).collect();
    let fixture = Arc::new(Fixture {
        graph,
        base_url: base.clone(),
    });
    let handle = tokio::spawn(async move {
        let _ = serve(listener, fixture).await;
    });
    (base, paths, handle)
}

async fn spawn(app: axum::Router) -> (String, JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (base, handle)
}

// ---- ordinary responses ----

#[tokio::test]
async fn a_page_comes_back_with_its_body_and_status() {
    let (base, paths, server) = spawn_fixture().await;
    let f = fetcher(FetchConfig::default());

    let got = f.fetch(&url(&base, &paths[1])).await.unwrap();
    assert_eq!(got.status, 200);
    assert!(!got.truncated);
    assert!(
        String::from_utf8_lossy(&got.body).contains("<title>"),
        "expected a rendered page"
    );
    server.abort();
}

#[tokio::test]
async fn a_404_is_a_result_rather_than_an_error() {
    // A broken link is the finding. Reporting it as an error would lose it.
    let (base, _paths, server) = spawn_fixture().await;
    let f = fetcher(FetchConfig::default());

    let got = f.fetch(&url(&base, "/status/404")).await.unwrap();
    assert_eq!(got.status, 404);
    server.abort();
}

#[tokio::test]
async fn a_redirect_comes_back_as_itself() {
    // The pool fetches exactly one URL. Walking the chain is T1.7's job, and
    // a pool that followed hops silently would destroy the chain.
    let (base, _paths, server) = spawn_fixture().await;
    let f = fetcher(FetchConfig::default());

    let got = f.fetch(&url(&base, "/redirect-chain/3")).await.unwrap();
    assert!(got.status.is_redirection(), "got {}", got.status);
    assert!(got.headers.contains_key("location"));
    server.abort();
}

// ---- robots ----

#[tokio::test]
async fn a_disallowed_url_is_refused_without_being_requested() {
    // The assertion that matters is the second one: not merely that we report
    // a denial, but that no request ever reached the server.
    let hits: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let app = axum::Router::new()
        .route(
            "/robots.txt",
            get(|| async { "User-agent: *\nDisallow: /private/\n" }),
        )
        .route(
            "/private/{page}",
            get(|State(hits): State<Arc<Mutex<Vec<String>>>>| async move {
                hits.lock().unwrap().push("/private/".into());
                "secret"
            }),
        )
        .with_state(hits.clone());
    let (base, server) = spawn(app).await;
    let f = fetcher(FetchConfig::default());

    let err = f.fetch(&url(&base, "/private/secret")).await.unwrap_err();
    assert!(matches!(err, FetchError::RobotsDenied(_)), "{err}");
    assert!(
        hits.lock().unwrap().is_empty(),
        "a disallowed path was fetched anyway"
    );
    server.abort();
}

// ---- retries ----

/// Answers 503 for the first `failures` requests, then 200. Counts every
/// request that arrives.
fn flaky(failures: usize) -> (axum::Router, Arc<AtomicUsize>) {
    let seen = Arc::new(AtomicUsize::new(0));
    let app = axum::Router::new()
        .route(
            "/flaky",
            get(
                move |State((seen, failures)): State<(Arc<AtomicUsize>, usize)>| async move {
                    let n = seen.fetch_add(1, Ordering::SeqCst);
                    let r: Response = if n < failures {
                        StatusCode::SERVICE_UNAVAILABLE.into_response()
                    } else {
                        "recovered".into_response()
                    };
                    r
                },
            ),
        )
        .with_state((seen.clone(), failures));
    (app, seen)
}

#[tokio::test]
async fn a_transient_failure_is_retried_until_it_succeeds() {
    let (app, seen) = flaky(2);
    let (base, server) = spawn(app).await;
    let f = fetcher(FetchConfig::default());

    let got = f.fetch(&url(&base, "/flaky")).await.unwrap();
    assert_eq!(got.status, 200);
    assert_eq!(seen.load(Ordering::SeqCst), 3, "expected two retries");
    assert_eq!(got.body, b"recovered");
    server.abort();
}

#[tokio::test]
async fn retries_that_run_out_hand_back_the_failing_response() {
    // A server that stayed unavailable is a finding about that server, not an
    // error that erases it.
    let (app, seen) = flaky(usize::MAX);
    let (base, server) = spawn(app).await;
    let f = fetcher(FetchConfig::default());

    let got = f.fetch(&url(&base, "/flaky")).await.unwrap();
    assert_eq!(got.status, 503);
    assert_eq!(seen.load(Ordering::SeqCst), 3, "the attempt limit is 3");
    server.abort();
}

#[tokio::test]
async fn a_retry_waits_for_the_hosts_slot_and_not_only_the_backoff() {
    // The trap this pool exists to avoid. A retry is another request to a host
    // that just said it was struggling; if the backoff bypassed the limiter,
    // a wave of retries would arrive as a burst on exactly the wrong server.
    const SPACING: Duration = Duration::from_millis(600);
    let (app, _) = flaky(1);
    let (base, server) = spawn(app).await;
    let f = fetcher(FetchConfig {
        default_delay: SPACING,
        ..FetchConfig::default()
    });

    let start = std::time::Instant::now();
    let got = f.fetch(&url(&base, "/flaky")).await.unwrap();
    let elapsed = start.elapsed();

    assert_eq!(got.status, 200);
    assert!(
        elapsed >= SPACING,
        "the retry skipped the host's spacing: {elapsed:?}"
    );
    server.abort();
}

// ---- failures with no response at all ----

#[tokio::test]
async fn a_host_that_cannot_be_reached_is_not_crawled() {
    // Surfaces as a robots denial rather than a transport error, and that is
    // RFC 9309 working as intended: robots.txt was unreachable, so the rules
    // are undefined and nothing on the host may be fetched. Worth knowing when
    // reading a crawl report — see PLAN.md T1.8 on telling the two apart.
    let f = fetcher(FetchConfig {
        timeout: Duration::from_millis(200),
        ..FetchConfig::default()
    });
    let u = CrawlUrl::parse("http://127.0.0.1:1/page").unwrap();

    let err = f.fetch(&u).await.unwrap_err();
    assert!(matches!(err, FetchError::RobotsDenied(_)), "{err}");
}

#[tokio::test]
async fn a_response_slower_than_the_timeout_is_abandoned() {
    // One hung connection must not hold a fetch slot for the rest of a crawl.
    let (base, _paths, server) = spawn_fixture().await;
    let f = fetcher(FetchConfig {
        timeout: Duration::from_millis(150),
        ..FetchConfig::default()
    });

    let err = f.fetch(&url(&base, "/slow/5000")).await.unwrap_err();
    assert!(matches!(err, FetchError::Transport { .. }), "{err}");
    server.abort();
}

// ---- oversized bodies ----

#[tokio::test]
async fn an_oversized_body_is_truncated_rather_than_buffered() {
    // 8 MB served against a 64 KB ceiling. The point is that memory tracks the
    // ceiling and not the response.
    const CAP: usize = 64 * 1024;
    let (base, _paths, server) = spawn_fixture().await;
    let f = fetcher(FetchConfig {
        max_body_bytes: CAP,
        ..FetchConfig::default()
    });

    let got = f.fetch(&url(&base, "/huge/8")).await.unwrap();
    assert!(got.truncated, "an 8 MB body should have hit the ceiling");
    assert_eq!(got.body.len(), CAP);
    server.abort();
}
