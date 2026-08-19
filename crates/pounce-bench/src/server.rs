//! The fixture HTTP server.
//!
//! Pages resolve through a hash lookup in a fallback handler rather than being
//! registered as individual routes. A 100k-route router would be slow to build
//! and large to hold, and the lookup is O(1) either way.

use crate::graph::SiteGraph;
use crate::pathological::{
    Redirect, clamp_delay_ms, clamp_hops, huge_html, malformed_html, redirect_chain_target,
    redirect_loop_target,
};
use crate::render::render_page;
use axum::{
    Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::get,
};
use std::sync::Arc;
use std::time::Duration;

pub struct Fixture {
    pub graph: SiteGraph,
    pub base_url: String,
}

type Shared = Arc<Fixture>;

/// NOTE: axum 0.8 uses `{param}` path syntax. The 0.7 `:param` form panics at
/// router construction.
pub fn app(fixture: Shared) -> Router {
    Router::new()
        .route("/robots.txt", get(robots))
        .route("/sitemap.xml", get(sitemap))
        .route("/malformed", get(malformed))
        .route("/slow/{ms}", get(slow))
        .route("/huge/{mb}", get(huge))
        .route("/status/{code}", get(status))
        .route("/redirect-chain/{n}", get(chain))
        .route("/redirect-loop/{size}/{step}", get(loop_))
        .fallback(page)
        .with_state(fixture)
}

pub async fn serve(listener: tokio::net::TcpListener, fixture: Shared) -> anyhow::Result<()> {
    axum::serve(listener, app(fixture)).await?;
    Ok(())
}

fn html(body: String) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        "text/html; charset=utf-8".parse().unwrap(),
    );
    (StatusCode::OK, headers, body).into_response()
}

async fn page(State(fx): State<Shared>, uri: Uri) -> Response {
    match fx.graph.lookup(uri.path()) {
        Some(id) => html(render_page(&fx.graph, id, &fx.base_url)),
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

async fn robots(State(fx): State<Shared>) -> Response {
    let body = format!(
        "User-agent: *\nDisallow: /private/\nAllow: /\n\nSitemap: {}/sitemap.xml\n",
        fx.base_url
    );
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        body,
    )
        .into_response()
}

async fn sitemap(State(fx): State<Shared>) -> Response {
    let mut s = String::with_capacity(fx.graph.nodes.len() * 90);
    s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    s.push_str("<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n");
    for node in fx.graph.nodes.iter().filter(|n| !n.noindex) {
        s.push_str("  <url><loc>");
        s.push_str(&fx.base_url);
        s.push_str(&node.path);
        s.push_str("</loc></url>\n");
    }
    s.push_str("</urlset>\n");
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/xml")],
        s,
    )
        .into_response()
}

async fn malformed() -> Response {
    html(malformed_html())
}

async fn slow(Path(ms): Path<u64>) -> Response {
    tokio::time::sleep(Duration::from_millis(clamp_delay_ms(ms))).await;
    html(
        "<!DOCTYPE html>\n<html><head><title>Slow</title></head><body><h1>Slow</h1></body></html>"
            .into(),
    )
}

async fn huge(Path(mb): Path<usize>) -> Response {
    html(huge_html(mb))
}

async fn status(Path(code): Path<u16>) -> Response {
    let status = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    (status, format!("status {code}")).into_response()
}

async fn chain(Path(n): Path<u32>) -> Response {
    match redirect_chain_target(clamp_hops(n)) {
        Redirect::Terminal => html(
            "<!DOCTYPE html>\n<html><head><title>Chain end</title></head><body><h1>Chain end</h1></body></html>"
                .into(),
        ),
        Redirect::To(next) => {
            (StatusCode::MOVED_PERMANENTLY, [(header::LOCATION, next)]).into_response()
        }
    }
}

async fn loop_(Path((size, step)): Path<(u32, u32)>) -> Response {
    let next = redirect_loop_target(step, clamp_hops(size).max(2));
    (StatusCode::FOUND, [(header::LOCATION, next)]).into_response()
}
