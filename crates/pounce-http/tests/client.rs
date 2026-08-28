//! The two properties of the shared client that are invariants rather than
//! preferences: it says who it is, and it does not follow redirects.

use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::get;
use pounce_bench::graph::{GraphSpec, SiteGraph};
use pounce_bench::server::{Fixture, serve};
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn every_request_says_who_it_is() {
    // Recorded from the server side, because a user-agent that is merely
    // configured is not a user-agent that arrives.
    let seen: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let app = axum::Router::new()
        .route(
            "/",
            get(
                |State(seen): State<Arc<Mutex<Option<String>>>>, headers: HeaderMap| async move {
                    let ua = headers
                        .get(axum::http::header::USER_AGENT)
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_string);
                    *seen.lock().unwrap() = ua;
                    "ok"
                },
            ),
        )
        .with_state(seen.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    pounce_http::client()
        .unwrap()
        .get(&base)
        .send()
        .await
        .unwrap();
    server.abort();

    let ua = seen.lock().unwrap().clone().expect("no User-Agent arrived");
    assert_eq!(ua, pounce_http::USER_AGENT);
    assert!(ua.starts_with(pounce_http::PRODUCT_TOKEN), "{ua}");
    assert!(ua.contains("(+http"), "an anonymous crawler is a liability");
}

#[tokio::test]
async fn redirects_are_returned_rather_than_followed() {
    // The chain is the thing under audit. A client that follows it hands back
    // a 200 and loses every hop.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let fixture = Arc::new(Fixture::new(
        SiteGraph::generate(&GraphSpec {
            seed: 5,
            page_count: 50,
            ..GraphSpec::default()
        }),
        base.clone(),
    ));
    let server = tokio::spawn(async move {
        let _ = serve(listener, fixture).await;
    });

    let resp = pounce_http::client()
        .unwrap()
        .get(format!("{base}/redirect-chain/3"))
        .send()
        .await
        .unwrap();

    assert!(
        resp.status().is_redirection(),
        "expected a redirect, got {}",
        resp.status()
    );
    assert!(resp.headers().contains_key(axum::http::header::LOCATION));
    server.abort();
}
