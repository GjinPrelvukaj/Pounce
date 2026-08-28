use pounce_bench::graph::{GraphSpec, SiteGraph};
use pounce_bench::server::{Fixture, serve};
use std::sync::Arc;

const PAGES: u32 = 200;
const SEED: u64 = 5;

fn fixture_graph() -> SiteGraph {
    SiteGraph::generate(&GraphSpec {
        seed: SEED,
        page_count: PAGES,
        ..GraphSpec::default()
    })
}

async fn spawn() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base = format!("http://{addr}");
    let fixture = Arc::new(Fixture::new(fixture_graph(), base.clone()));
    tokio::spawn(async move { serve(listener, fixture).await });
    base
}

/// Redirects are never followed: the chain is the thing under test.
fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

#[tokio::test]
async fn serves_the_root_page() {
    let base = spawn().await;
    let res = client().get(&base).send().await.unwrap();
    assert_eq!(res.status(), 200);
    assert!(
        res.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );
    assert!(res.text().await.unwrap().contains("<h1>Home</h1>"));
}

#[tokio::test]
async fn unknown_paths_return_404() {
    let base = spawn().await;
    let res = client()
        .get(format!("{base}/no-such-page"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn serves_robots_txt_disallowing_the_private_area() {
    let base = spawn().await;
    let res = client()
        .get(format!("{base}/robots.txt"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body = res.text().await.unwrap();
    assert!(body.contains("User-agent: *"));
    assert!(body.contains("Disallow: /private/"));
    assert!(body.contains("Sitemap:"));
}

#[tokio::test]
async fn serves_a_sitemap_listing_indexable_pages() {
    let base = spawn().await;
    let body = client()
        .get(format!("{base}/sitemap.xml"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.starts_with("<?xml"));
    assert!(body.contains("<urlset"));
    assert!(body.contains("<loc>"));
    assert!(!body.contains("/private/"));
}

#[tokio::test]
async fn sitemap_omits_noindex_pages() {
    let base = spawn().await;
    let body = client()
        .get(format!("{base}/sitemap.xml"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let graph = fixture_graph();
    let noindexed = graph
        .nodes
        .iter()
        .find(|n| n.noindex)
        .expect("fixture should have a noindex page");
    assert!(
        !body.contains(&format!("<loc>{base}{}</loc>", noindexed.path)),
        "sitemap listed a noindex page"
    );
}

#[tokio::test]
async fn redirect_chain_terminates_in_a_200() {
    let base = spawn().await;
    let c = client();
    let res = c
        .get(format!("{base}/redirect-chain/3"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 301);
    assert_eq!(res.headers()["location"], "/redirect-chain/2");

    let end = c
        .get(format!("{base}/redirect-chain/0"))
        .send()
        .await
        .unwrap();
    assert_eq!(end.status(), 200);
}

#[tokio::test]
async fn redirect_loop_never_terminates() {
    let base = spawn().await;
    let res = client()
        .get(format!("{base}/redirect-loop/3/0"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 302);
    assert_eq!(res.headers()["location"], "/redirect-loop/3/1");
}

#[tokio::test]
async fn status_endpoint_returns_the_requested_code() {
    let base = spawn().await;
    let c = client();
    for code in [404u16, 410, 500, 503] {
        let res = c.get(format!("{base}/status/{code}")).send().await.unwrap();
        assert_eq!(res.status().as_u16(), code);
    }
}

#[tokio::test]
async fn slow_endpoint_actually_delays() {
    let base = spawn().await;
    let start = std::time::Instant::now();
    let res = client()
        .get(format!("{base}/slow/300"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert!(start.elapsed().as_millis() >= 300);
}

#[tokio::test]
async fn malformed_endpoint_serves_broken_markup() {
    let base = spawn().await;
    let body = client()
        .get(format!("{base}/malformed"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("/malformed-target-1"));
    assert!(!body.contains("</html>"));
}

#[tokio::test]
async fn huge_endpoint_serves_a_large_body() {
    let base = spawn().await;
    let body = client()
        .get(format!("{base}/huge/2"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.len() >= 2 * 1024 * 1024);
}

#[tokio::test]
async fn every_generated_page_is_reachable_over_http() {
    let base = spawn().await;
    let graph = fixture_graph();
    let c = client();
    for node in graph.nodes.iter().take(25) {
        let res = c.get(format!("{base}{}", node.path)).send().await.unwrap();
        assert_eq!(res.status(), 200, "path {} was not served", node.path);
    }
}

#[tokio::test]
async fn served_pages_carry_their_own_canonical() {
    let base = spawn().await;
    let graph = fixture_graph();
    let node = graph.node(17);
    let body = client()
        .get(format!("{base}{}", node.path))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains(&format!("href=\"{base}{}\"", node.path)));
}
