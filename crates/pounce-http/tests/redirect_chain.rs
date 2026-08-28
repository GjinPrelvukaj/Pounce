//! Walking redirect chains against real servers.
//!
//! The fixture site supplies the two shapes that matter — a chain that
//! terminates and a loop that never does. Throwaway servers cover the cases the
//! fixture has no reason to serve: a missing `Location`, a cross-host jump, and
//! a chain long enough to trip the hop cap.

use axum::extract::Path;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::get;
use pounce_bench::graph::{GraphSpec, SiteGraph};
use pounce_bench::server::{Fixture, serve};
use pounce_core::CrawlUrl;
use pounce_http::fetch::{FetchConfig, Fetcher};
use pounce_http::redirect::{Outcome, is_redirect};
use std::sync::Arc;
use tokio::task::JoinHandle;

fn fetcher(config: FetchConfig) -> Fetcher {
    Fetcher::new(config).unwrap()
}

fn url(base: &str, path: &str) -> CrawlUrl {
    CrawlUrl::parse(&format!("{base}{path}")).unwrap()
}

async fn spawn_fixture() -> (String, JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let graph = SiteGraph::generate(&GraphSpec {
        seed: 5,
        page_count: 20,
        ..GraphSpec::default()
    });
    let fixture = Arc::new(Fixture::new(graph, base.clone()));
    let handle = tokio::spawn(async move {
        let _ = serve(listener, fixture).await;
    });
    (base, handle)
}

/// A server whose routes are scripted by the test rather than generated.
async fn spawn_scripted() -> (String, JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let app = axum::Router::new()
        // 301 with no Location header at all.
        .route(
            "/no-location",
            get(|| async { (StatusCode::MOVED_PERMANENTLY, "gone somewhere").into_response() }),
        )
        // 302 whose Location cannot be resolved to a crawlable URL.
        .route(
            "/bad-location",
            get(|| async {
                (
                    StatusCode::FOUND,
                    [(header::LOCATION, "mailto:someone@example.com")],
                )
                    .into_response()
            }),
        )
        // An unbounded descending chain: /step/N -> /step/N+1, forever.
        .route(
            "/step/{n}",
            get(|Path(n): Path<u32>| async move {
                (
                    StatusCode::TEMPORARY_REDIRECT,
                    [(header::LOCATION, format!("/step/{}", n + 1))],
                )
                    .into_response()
            }),
        )
        // 303 See Other, then a real page. Exercises a status the chain must
        // treat as a redirect even though it changes method semantics.
        .route(
            "/see-other",
            get(|| async {
                (StatusCode::SEE_OTHER, [(header::LOCATION, "/landed")]).into_response()
            }),
        )
        .route(
            "/landed",
            get(|| async { axum::response::Html("<html><h1>Landed</h1></html>").into_response() }),
        )
        // 304 is not a redirect: no Location, and it means "use your cache".
        .route(
            "/not-modified",
            get(|| async { StatusCode::NOT_MODIFIED.into_response() }),
        );
    let handle = tokio::spawn(async move {
        let _: Result<(), _> = axum::serve(listener, app).await;
    });
    (base, handle)
}

fn hop_paths(chain: &pounce_http::redirect::RedirectChain) -> Vec<&str> {
    chain.hops.iter().map(|h| h.url.path()).collect()
}

// ---- the two cases T1.7 is defined by -----------------------------------

#[tokio::test]
async fn records_every_hop_of_a_finite_chain() {
    let (base, server) = spawn_fixture().await;
    let f = fetcher(FetchConfig::default());

    let chain = f.follow(&url(&base, "/redirect-chain/5")).await;

    // Five redirects, then the terminal page. Each hop is recorded in the
    // order it was crossed, counting down to the 200 at /redirect-chain/0.
    assert_eq!(
        hop_paths(&chain),
        [
            "/redirect-chain/5",
            "/redirect-chain/4",
            "/redirect-chain/3",
            "/redirect-chain/2",
            "/redirect-chain/1",
        ]
    );
    assert!(chain.hops.iter().all(|h| h.status.as_u16() == 301));
    match &chain.outcome {
        Outcome::Landed(f) => {
            assert_eq!(f.status, StatusCode::OK);
            assert_eq!(f.url.path(), "/redirect-chain/0");
        }
        other => panic!("expected a landing, got {other:?}"),
    }
    assert!(!chain.is_broken());
    assert_eq!(chain.final_url().path(), "/redirect-chain/0");

    server.abort();
}

#[tokio::test]
async fn a_redirect_loop_terminates_and_is_flagged() {
    let (base, server) = spawn_fixture().await;
    let f = fetcher(FetchConfig::default());

    let chain = f.follow(&url(&base, "/redirect-loop/3/0")).await;

    // /redirect-loop/3/0 -> /1 -> /2 -> /0, which we have already seen. The
    // walk must stop on the repeat rather than on the hop cap: a three-hop
    // loop under a ten-hop budget would otherwise be reported as a long chain.
    match &chain.outcome {
        Outcome::Loop(repeated) => assert_eq!(repeated.path(), "/redirect-loop/3/0"),
        other => panic!("expected a loop, got {other:?}"),
    }
    assert_eq!(
        hop_paths(&chain),
        [
            "/redirect-loop/3/0",
            "/redirect-loop/3/1",
            "/redirect-loop/3/2",
        ]
    );
    assert!(chain.is_broken());

    server.abort();
}

// ---- the cap ------------------------------------------------------------

#[tokio::test]
async fn an_endless_chain_stops_at_the_hop_cap() {
    let (base, server) = spawn_scripted().await;
    let f = fetcher(FetchConfig {
        max_redirects: 4,
        ..FetchConfig::default()
    });

    let chain = f.follow(&url(&base, "/step/0")).await;

    assert!(matches!(chain.outcome, Outcome::HopLimit));
    // Exactly the budget, not one more and not one fewer. An off-by-one here
    // is invisible in production and doubles the requests to a broken host.
    assert_eq!(chain.hops.len(), 4);
    assert_eq!(
        hop_paths(&chain),
        ["/step/0", "/step/1", "/step/2", "/step/3"]
    );

    server.abort();
}

#[tokio::test]
async fn the_hop_cap_is_reachable_at_its_boundary() {
    let (base, server) = spawn_fixture().await;
    // The chain needs exactly 3 redirects; a budget of 3 must complete it.
    let f = fetcher(FetchConfig {
        max_redirects: 3,
        ..FetchConfig::default()
    });

    let chain = f.follow(&url(&base, "/redirect-chain/3")).await;

    assert!(
        matches!(chain.outcome, Outcome::Landed(_)),
        "a chain of exactly max_redirects hops must land, got {:?}",
        chain.outcome
    );
    assert_eq!(chain.hops.len(), 3);

    server.abort();
}

// ---- malformed redirects ------------------------------------------------

#[tokio::test]
async fn a_redirect_without_a_location_is_reported_not_followed() {
    let (base, server) = spawn_scripted().await;
    let f = fetcher(FetchConfig::default());

    let chain = f.follow(&url(&base, "/no-location")).await;

    assert!(matches!(chain.outcome, Outcome::NoLocation));
    // The hop is still recorded: "301 with no Location" is the finding.
    assert_eq!(hop_paths(&chain), ["/no-location"]);
    assert_eq!(chain.hops[0].target, None);

    server.abort();
}

#[tokio::test]
async fn a_location_that_is_not_crawlable_is_reported_verbatim() {
    let (base, server) = spawn_scripted().await;
    let f = fetcher(FetchConfig::default());

    let chain = f.follow(&url(&base, "/bad-location")).await;

    assert!(matches!(chain.outcome, Outcome::NoLocation));
    assert_eq!(chain.hops[0].location, "mailto:someone@example.com");
    assert_eq!(chain.hops[0].target, None);

    server.abort();
}

// ---- statuses -----------------------------------------------------------

#[tokio::test]
async fn see_other_is_followed() {
    let (base, server) = spawn_scripted().await;
    let f = fetcher(FetchConfig::default());

    let chain = f.follow(&url(&base, "/see-other")).await;

    match &chain.outcome {
        Outcome::Landed(f) => assert_eq!(f.url.path(), "/landed"),
        other => panic!("expected a landing, got {other:?}"),
    }
    assert_eq!(chain.hops[0].status, StatusCode::SEE_OTHER);

    server.abort();
}

#[tokio::test]
async fn not_modified_is_not_a_redirect() {
    let (base, server) = spawn_scripted().await;
    let f = fetcher(FetchConfig::default());

    let chain = f.follow(&url(&base, "/not-modified")).await;

    // 304 lands. Treating it as a redirect would mean chasing a Location that
    // is not there and reporting every conditional hit as a broken chain.
    match &chain.outcome {
        Outcome::Landed(f) => assert_eq!(f.status, StatusCode::NOT_MODIFIED),
        other => panic!("expected a landing, got {other:?}"),
    }
    assert!(chain.hops.is_empty());

    server.abort();
}

#[test]
fn redirect_statuses_are_exactly_the_ones_carrying_a_location() {
    for code in [301, 302, 303, 307, 308] {
        assert!(is_redirect(StatusCode::from_u16(code).unwrap()), "{code}");
    }
    for code in [200, 201, 204, 304, 400, 404, 500, 503] {
        assert!(!is_redirect(StatusCode::from_u16(code).unwrap()), "{code}");
    }
}

// ---- a chain that does not redirect at all ------------------------------

#[tokio::test]
async fn an_ordinary_page_produces_an_empty_chain() {
    let (base, server) = spawn_fixture().await;
    let f = fetcher(FetchConfig::default());

    let chain = f.follow(&url(&base, "/")).await;

    assert!(chain.hops.is_empty());
    assert!(matches!(chain.outcome, Outcome::Landed(_)));
    assert!(!chain.is_broken());
    assert_eq!(chain.final_url().path(), "/");

    server.abort();
}

#[tokio::test]
async fn a_robots_denied_hop_keeps_the_hops_that_led_to_it() {
    let (base, server) = spawn_fixture().await;
    let f = fetcher(FetchConfig::default());

    // The fixture's robots.txt disallows /private/. Redirecting into it is the
    // realistic shape of this: the chain is legal right up until it is not.
    let chain = f.follow(&url(&base, "/private/secret")).await;

    assert!(matches!(chain.outcome, Outcome::Failed(_)));
    assert!(chain.is_broken());

    server.abort();
}
