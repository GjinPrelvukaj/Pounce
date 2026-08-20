//! What a fetch records about a response beyond its body.
//!
//! Two things are being pinned down here. The metadata itself — status,
//! timing, size, content type — and the T1.6a reporting split: a host that is
//! down must not be indistinguishable from a host that banned us.

use axum::http::{HeaderValue, StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::get;
use pounce_core::CrawlUrl;
use pounce_http::fetch::{FetchConfig, FetchError, Fetcher};
use pounce_http::robots::Access;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;

use pounce_bench::graph::{GraphSpec, SiteGraph};
use pounce_bench::server::{Fixture, serve};

fn fetcher(config: FetchConfig) -> Fetcher {
    Fetcher::new(config).unwrap()
}

fn url(base: &str, path: &str) -> CrawlUrl {
    CrawlUrl::parse(&format!("{base}{path}")).unwrap()
}

async fn spawn_fixture() -> (String, Vec<String>, JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let graph = SiteGraph::generate(&GraphSpec {
        seed: 5,
        page_count: 30,
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

/// Serves responses with content types the fixture site has no reason to send.
async fn spawn_typed() -> (String, JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());

    fn typed(ct: &'static str, body: &'static str) -> axum::routing::MethodRouter {
        get(move || async move {
            ([(header::CONTENT_TYPE, HeaderValue::from_static(ct))], body).into_response()
        })
    }

    let app = axum::Router::new()
        .route("/upper", typed("Text/HTML; charset=UTF-8", "<html></html>"))
        .route("/xhtml", typed("application/xhtml+xml", "<html></html>"))
        .route("/json", typed("application/json", "{}"))
        .route("/pdf", typed("application/pdf", "%PDF-"))
        .route(
            "/spaced",
            typed("text/html ;  charset = iso-8859-1", "<html>"),
        )
        .route("/bare", typed("text/html", "<html></html>"))
        // No Content-Type at all.
        .route(
            "/untyped",
            get(|| async {
                let mut r = "some bytes".into_response();
                r.headers_mut().remove(header::CONTENT_TYPE);
                r
            }),
        )
        // robots.txt exists so this server does not read as unreachable.
        .route("/robots.txt", get(|| async { "User-agent: *\nAllow: /\n" }));

    let handle = tokio::spawn(async move {
        let _: Result<(), _> = axum::serve(listener, app).await;
    });
    (base, handle)
}

// ---- T1.6a: telling "banned" apart from "down" -------------------------

#[tokio::test]
async fn an_unreachable_host_is_not_reported_as_a_robots_ban() {
    // The behaviour is unchanged and correct — RFC 9309 makes an unreadable
    // robots.txt a complete disallow. What changes is the report: nothing on
    // this origin may be fetched *because the host did not answer*, which is
    // the opposite of a site that deliberately banned us.
    let f = fetcher(FetchConfig {
        timeout: Duration::from_millis(200),
        ..FetchConfig::default()
    });
    let u = CrawlUrl::parse("http://127.0.0.1:1/page").unwrap();

    let err = f.fetch(&u).await.unwrap_err();
    match err {
        FetchError::RobotsUnreadable { url, reason } => {
            assert_eq!(url, u);
            assert!(!reason.is_empty(), "the reason is the whole point");
            // Not reqwest's Display: the URL is already the row being read.
            assert!(!reason.contains("127.0.0.1"), "reason was {reason:?}");
        }
        other => panic!("expected RobotsUnreadable, got {other:?}"),
    }
}

#[tokio::test]
async fn a_genuine_disallow_is_still_reported_as_a_ban() {
    let (base, _paths, server) = spawn_fixture().await;
    let f = fetcher(FetchConfig::default());

    // The fixture's robots.txt is readable and disallows /private/.
    let err = f.fetch(&url(&base, "/private/secret")).await.unwrap_err();
    assert!(matches!(err, FetchError::RobotsDenied(_)), "{err}");

    server.abort();
}

#[tokio::test]
async fn the_two_refusals_are_distinguishable_without_fetching() {
    let (base, _paths, server) = spawn_fixture().await;
    let client = pounce_http::client().unwrap();
    let cache = pounce_http::robots::RobotsCache::new(pounce_http::PRODUCT_TOKEN);

    assert_eq!(
        cache.access(&client, &url(&base, "/")).await,
        Access::Allowed
    );
    assert_eq!(
        cache.access(&client, &url(&base, "/private/x")).await,
        Access::Disallowed
    );

    let dead = CrawlUrl::parse("http://127.0.0.1:1/x").unwrap();
    assert!(matches!(
        cache.access(&client, &dead).await,
        Access::Unreadable(_)
    ));

    server.abort();
}

// ---- content type -------------------------------------------------------

#[tokio::test]
async fn content_type_is_split_into_media_type_and_charset() {
    let (base, server) = spawn_typed().await;
    let f = fetcher(FetchConfig::default());

    let got = f.fetch(&url(&base, "/upper")).await.unwrap();
    // Kept verbatim for the report, normalised for decisions.
    assert_eq!(got.content_type(), Some("Text/HTML; charset=UTF-8"));
    assert_eq!(got.mime().as_deref(), Some("text/html"));
    assert_eq!(got.charset().as_deref(), Some("utf-8"));
    assert!(got.is_html());

    server.abort();
}

#[tokio::test]
async fn whitespace_around_the_parameters_is_tolerated() {
    let (base, server) = spawn_typed().await;
    let f = fetcher(FetchConfig::default());

    let got = f.fetch(&url(&base, "/spaced")).await.unwrap();
    assert_eq!(got.mime().as_deref(), Some("text/html"));
    assert_eq!(got.charset().as_deref(), Some("iso-8859-1"));

    server.abort();
}

#[tokio::test]
async fn a_type_without_parameters_has_no_charset() {
    let (base, server) = spawn_typed().await;
    let f = fetcher(FetchConfig::default());

    let got = f.fetch(&url(&base, "/bare")).await.unwrap();
    assert_eq!(got.mime().as_deref(), Some("text/html"));
    assert_eq!(got.charset(), None);
    assert!(got.is_html());

    server.abort();
}

#[tokio::test]
async fn xhtml_is_html_and_json_and_pdf_are_not() {
    let (base, server) = spawn_typed().await;
    let f = fetcher(FetchConfig::default());

    assert!(f.fetch(&url(&base, "/xhtml")).await.unwrap().is_html());
    assert!(!f.fetch(&url(&base, "/json")).await.unwrap().is_html());
    assert!(!f.fetch(&url(&base, "/pdf")).await.unwrap().is_html());

    server.abort();
}

#[tokio::test]
async fn a_response_with_no_content_type_is_not_guessed_at() {
    let (base, server) = spawn_typed().await;
    let f = fetcher(FetchConfig::default());

    let got = f.fetch(&url(&base, "/untyped")).await.unwrap();
    assert_eq!(got.content_type(), None);
    assert_eq!(got.mime(), None);
    // Not sniffed from the body. A report that guesses is worse than one that
    // says the server declared nothing.
    assert!(!got.is_html());

    server.abort();
}

// ---- size and timing ----------------------------------------------------

#[tokio::test]
async fn size_is_the_bytes_actually_read() {
    let (base, paths, server) = spawn_fixture().await;
    let f = fetcher(FetchConfig::default());

    let got = f.fetch(&url(&base, &paths[1])).await.unwrap();
    assert_eq!(got.size(), got.body.len());
    assert!(got.size() > 0);
    // The fixture serves a fixed-length body, so the declaration must match
    // what arrived. A mismatch here is a truncated transfer.
    assert_eq!(got.declared_length, Some(got.size() as u64));

    server.abort();
}

#[tokio::test]
async fn a_truncated_body_reads_short_of_what_was_declared() {
    let (base, _paths, server) = spawn_fixture().await;
    let f = fetcher(FetchConfig {
        max_body_bytes: 4096,
        ..FetchConfig::default()
    });

    let got = f.fetch(&url(&base, "/huge/8")).await.unwrap();
    assert!(got.truncated);
    assert_eq!(got.size(), 4096);
    // Keeping the declaration is what makes the truncation legible: the report
    // can say 4 KB of a declared 8 MB rather than just "4 KB".
    assert!(got.declared_length.unwrap() > got.size() as u64);

    server.abort();
}

#[tokio::test]
async fn headers_arrive_before_the_body_finishes() {
    let (base, paths, server) = spawn_fixture().await;
    let f = fetcher(FetchConfig::default());

    let got = f.fetch(&url(&base, &paths[2])).await.unwrap();
    assert!(
        got.time_to_headers <= got.elapsed,
        "headers at {:?} cannot be later than the whole response at {:?}",
        got.time_to_headers,
        got.elapsed
    );

    server.abort();
}

#[tokio::test]
async fn a_slow_server_shows_up_as_slow_headers_not_a_slow_body() {
    let (base, _paths, server) = spawn_fixture().await;
    let f = fetcher(FetchConfig {
        timeout: Duration::from_secs(5),
        ..FetchConfig::default()
    });

    // /slow/300 stalls before responding at all, so the delay belongs to the
    // headers. This is the distinction the two timings exist to make.
    let got = f.fetch(&url(&base, "/slow/300")).await.unwrap();
    assert!(
        got.time_to_headers >= Duration::from_millis(250),
        "time_to_headers was {:?}",
        got.time_to_headers
    );

    server.abort();
}

// ---- status and headers are recorded, not interpreted -------------------

#[tokio::test]
async fn a_failing_status_is_still_a_recorded_response() {
    let (base, _paths, server) = spawn_fixture().await;
    let f = fetcher(FetchConfig::default());

    let got = f.fetch(&url(&base, "/status/404")).await.unwrap();
    assert_eq!(got.status, StatusCode::NOT_FOUND);
    assert!(got.declared_length.is_some());
    assert!(!got.headers.is_empty());

    server.abort();
}
