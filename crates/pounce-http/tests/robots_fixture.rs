//! `RobotsCache` against the real fixture server, which serves
//! `Disallow: /private/` under `User-agent: *`.

use pounce_bench::graph::{GraphSpec, SiteGraph};
use pounce_bench::server::{Fixture, serve};
use pounce_core::CrawlUrl;
use pounce_http::robots::RobotsCache;
use std::sync::Arc;
use tokio::task::JoinHandle;

const AGENT: &str = "PounceBot";

async fn spawn() -> (String, JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let fixture = Arc::new(Fixture {
        graph: SiteGraph::generate(&GraphSpec {
            seed: 5,
            page_count: 50,
            ..GraphSpec::default()
        }),
        base_url: base.clone(),
    });
    let handle = tokio::spawn(async move {
        let _ = serve(listener, fixture).await;
    });
    (base, handle)
}

/// The shared client: auto-redirect is disabled there, and robots.txt walks
/// its own hops deliberately.
fn client() -> reqwest::Client {
    pounce_http::client().unwrap()
}

fn url(base: &str, path: &str) -> CrawlUrl {
    CrawlUrl::parse(&format!("{base}{path}")).unwrap()
}

#[tokio::test]
async fn a_disallowed_fixture_path_is_refused_and_an_ordinary_one_is_not() {
    let (base, server) = spawn().await;
    let cache = RobotsCache::new(AGENT);
    let c = client();

    assert!(cache.is_allowed(&c, &url(&base, "/page/1")).await);
    assert!(!cache.is_allowed(&c, &url(&base, "/private/secret")).await);
    server.abort();
}

#[tokio::test]
async fn the_fixture_states_no_crawl_delay() {
    let (base, server) = spawn().await;
    let cache = RobotsCache::new(AGENT);
    let rules = cache.get(&client(), &url(&base, "/page/1")).await;
    assert_eq!(rules.crawl_delay(), None);
    server.abort();
}

#[tokio::test]
async fn rules_are_cached_per_origin_rather_than_refetched() {
    // Proved by killing the server after the first lookup. Without a cache the
    // second call would fail to connect and, per RFC 9309, deny everything.
    let (base, server) = spawn().await;
    let cache = RobotsCache::new(AGENT);
    let c = client();

    assert!(cache.is_allowed(&c, &url(&base, "/page/1")).await);
    server.abort();
    let _ = server.await;

    assert!(cache.is_allowed(&c, &url(&base, "/page/2")).await);
    assert!(!cache.is_allowed(&c, &url(&base, "/private/x")).await);
}

#[tokio::test]
async fn an_unreachable_host_denies_everything() {
    // Nothing is listening on this port, so the file is undefined rather than
    // absent, and the crawl must not proceed on the assumption it is welcome.
    let cache = RobotsCache::new(AGENT);
    let u = CrawlUrl::parse("http://127.0.0.1:1/page/1").unwrap();
    // Bounded so a platform that black-holes this port fails visibly instead
    // of hanging the suite. T1.6 owns the real per-request timeout.
    let allowed = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        cache.is_allowed(&client(), &u),
    )
    .await
    .expect("connection attempt should settle quickly");
    assert!(!allowed);
}

// ---- redirected robots.txt ----
//
// Not servable from the fixture site, which answers /robots.txt directly, so
// these stand up a throwaway server. The path matters: a site seeded on http
// that redirects everything to https reaches its rules only this way, and
// getting it wrong means silently ignoring robots.txt on a large slice of the
// web.

/// `/robots.txt` redirects to `/r/{hops-1}`, which counts down to `/r/0`, which
/// serves the rules. So `hops` is the number of redirects before the body.
async fn spawn_redirecting(hops: u32) -> (String, JoinHandle<()>) {
    use axum::extract::Path;
    use axum::response::{IntoResponse, Redirect, Response};
    use axum::routing::get;

    let app = axum::Router::new()
        .route(
            "/robots.txt",
            get(move || async move { Redirect::temporary(&format!("/r/{}", hops - 1)) }),
        )
        .route(
            "/r/{n}",
            get(|Path(n): Path<u32>| async move {
                let body: Response = match n {
                    0 => "User-agent: *
Disallow: /private/
"
                    .into_response(),
                    n => Redirect::temporary(&format!("/r/{}", n - 1)).into_response(),
                };
                body
            }),
        );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (base, handle)
}

#[tokio::test]
async fn a_redirected_robots_txt_is_followed_to_its_rules() {
    let (base, server) = spawn_redirecting(1).await;
    let cache = RobotsCache::new(AGENT);
    let c = client();

    assert!(!cache.is_allowed(&c, &url(&base, "/private/x")).await);
    assert!(cache.is_allowed(&c, &url(&base, "/page/1")).await);
    server.abort();
}

#[tokio::test]
async fn five_consecutive_redirects_are_still_followed() {
    // RFC 9309 asks for at least five.
    let (base, server) = spawn_redirecting(5).await;
    let cache = RobotsCache::new(AGENT);
    assert!(!cache.is_allowed(&client(), &url(&base, "/private/x")).await);
    server.abort();
}

#[tokio::test]
async fn giving_up_on_a_longer_redirect_chain_allows_the_crawl() {
    // Past the hop cap the file counts as unavailable, not as a block: a
    // misconfigured redirect must not silently stop a crawl.
    let (base, server) = spawn_redirecting(6).await;
    let cache = RobotsCache::new(AGENT);
    assert!(cache.is_allowed(&client(), &url(&base, "/private/x")).await);
    server.abort();
}
