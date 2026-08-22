//! `HEAD` checks: what the crawl learns about a URL it will never parse.
//!
//! Images are the reason this exists. A `<img src>` needs a status and a size
//! and nothing else, and downloading megabytes of JPEG to learn two numbers
//! would multiply a crawl's bytes for no finding it does not already have.

use axum::http::{HeaderValue, StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::{any, get};
use pounce_core::CrawlUrl;
use pounce_http::fetch::{FetchConfig, FetchError, Fetcher};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::task::JoinHandle;

/// Counts request methods, so "no body was transferred" is asserted against
/// what the server saw rather than against what the client says it did.
#[derive(Default)]
struct Seen {
    gets: AtomicUsize,
    heads: AtomicUsize,
}

async fn spawn() -> (String, Arc<Seen>, JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Seen::default());

    let counted = {
        let seen = Arc::clone(&seen);
        any(move |method: axum::http::Method| {
            let seen = Arc::clone(&seen);
            async move {
                match method {
                    axum::http::Method::HEAD => seen.heads.fetch_add(1, Ordering::SeqCst),
                    _ => seen.gets.fetch_add(1, Ordering::SeqCst),
                };
                (
                    [(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg"))],
                    vec![0u8; 4096],
                )
                    .into_response()
            }
        })
    };

    let app = axum::Router::new()
        .route("/photo.jpg", counted)
        .route("/gone.png", any(|| async { StatusCode::NOT_FOUND }))
        .route("/robots.txt", get(|| async { "User-agent: *\nAllow: /\n" }));

    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (base, seen, handle)
}

fn fetcher() -> Fetcher {
    Fetcher::new(FetchConfig::default()).unwrap()
}

fn url(base: &str, path: &str) -> CrawlUrl {
    CrawlUrl::parse(&format!("{base}{path}")).unwrap()
}

#[tokio::test]
async fn a_head_reports_status_length_and_type_without_fetching_a_body() {
    let (base, seen, handle) = spawn().await;
    let got = fetcher().head(&url(&base, "/photo.jpg")).await.unwrap();
    assert_eq!(got.status, StatusCode::OK);
    assert_eq!(got.content_length, Some(4096));
    assert_eq!(got.content_type.as_deref(), Some("image/jpeg"));

    // The point of the whole feature: the server saw a HEAD and no GET, so the
    // 4096 bytes were never transferred.
    assert_eq!(seen.heads.load(Ordering::SeqCst), 1);
    assert_eq!(seen.gets.load(Ordering::SeqCst), 0);
    handle.abort();
}

#[tokio::test]
async fn a_404_is_a_result_not_an_error() {
    // Same rule as `fetch`: a broken image is the finding, so it must come
    // back as Ok for the audit to see it at all.
    let (base, _seen, handle) = spawn().await;
    let got = fetcher().head(&url(&base, "/gone.png")).await.unwrap();
    assert_eq!(got.status, StatusCode::NOT_FOUND);
    handle.abort();
}

#[tokio::test]
async fn an_undeclared_length_is_unknown_rather_than_zero() {
    // Written on a raw socket rather than through axum: hyper computes a
    // Content-Length for any body whose size it knows, so a fixture that only
    // *removes* the header gets it back. The case being tested — a chunked or
    // streamed image — is common enough on real sites that it has to be
    // exercised over the wire and not just asserted about the parser.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let handle = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let mut buf = [0u8; 1024];
            let n = socket.read(&mut buf).await.unwrap_or(0);
            let body = if buf[..n].starts_with(b"GET /robots.txt") {
                "HTTP/1.1 200 OK\r\nContent-Length: 23\r\nConnection: close\r\n\r\n\
                 User-agent: *\nAllow: /\n"
                    .to_string()
            } else {
                // Chunked, so there is no length to declare.
                "HTTP/1.1 200 OK\r\nContent-Type: image/svg+xml\r\n\
                 Transfer-Encoding: chunked\r\nConnection: close\r\n\r\n\
                 6\r\n<svg/>\r\n0\r\n\r\n"
                    .to_string()
            };
            let _ = socket.write_all(body.as_bytes()).await;
        }
    });

    let got = fetcher().head(&url(&base, "/chunked.svg")).await.unwrap();
    assert_eq!(got.status, StatusCode::OK);
    assert_eq!(got.content_type.as_deref(), Some("image/svg+xml"));
    assert_eq!(
        got.content_length, None,
        "a server that declared nothing must not be reported as declaring 0"
    );
    handle.abort();
}

#[tokio::test]
async fn head_obeys_robots_txt_like_every_other_request() {
    // Politeness defaults are correctness. A second request path that skipped
    // robots.txt would be a hole in the whole guarantee, so this asserts the
    // shared path is really shared.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let app = axum::Router::new()
        .route(
            "/robots.txt",
            get(|| async { "User-agent: *\nDisallow: /private/\n" }),
        )
        .route("/private/x.png", any(|| async { "img" }));
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let err = fetcher()
        .head(&url(&base, "/private/x.png"))
        .await
        .unwrap_err();
    assert!(matches!(err, FetchError::RobotsDenied(_)));
    handle.abort();
}
