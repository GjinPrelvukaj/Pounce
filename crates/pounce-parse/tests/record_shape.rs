//! The record's invariants, before any extractor writes to it.

use pounce_core::CrawlUrl;
use pounce_http::fetch::{FetchConfig, Fetcher};
use pounce_parse::record::{Hreflang, Image, Link};
use pounce_parse::{MetaRobots, PageRecord};
use std::sync::Arc;
use tokio::task::JoinHandle;

use pounce_bench::graph::{GraphSpec, SiteGraph};
use pounce_bench::server::{Fixture, serve};

async fn spawn_fixture() -> (String, Vec<String>, JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let graph = SiteGraph::generate(&GraphSpec {
        seed: 5,
        page_count: 20,
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

// ---- seeding from a fetch ------------------------------------------------

#[tokio::test]
async fn a_seeded_record_carries_the_transport_facts_and_nothing_else() {
    let (base, paths, server) = spawn_fixture().await;
    let f = Fetcher::new(FetchConfig::default()).unwrap();
    let url = CrawlUrl::parse(&format!("{base}{}", paths[1])).unwrap();

    let fetched = f.fetch(&url).await.unwrap();
    let record = PageRecord::from_fetched(&fetched, 3, vec!["/old".into()]);

    assert_eq!(record.url, url);
    assert_eq!(record.status, 200);
    assert_eq!(record.depth, 3);
    assert_eq!(record.size, fetched.body.len());
    assert!(!record.truncated);
    assert_eq!(record.content_type.as_deref(), Some("text/html"));
    assert_eq!(record.redirect_chain, ["/old"]);

    // Extraction has not run. These must be empty rather than defaulted to
    // something that reads like a finding.
    assert_eq!(record.title, None);
    assert_eq!(record.canonical, None);
    assert_eq!(record.meta_robots, MetaRobots::default());
    assert!(record.links.is_empty());
    assert!(record.images.is_empty());
    assert_eq!(record.word_count, 0);

    server.abort();
}

#[tokio::test]
async fn timings_are_recorded_in_milliseconds() {
    let (base, paths, server) = spawn_fixture().await;
    let f = Fetcher::new(FetchConfig::default()).unwrap();
    let url = CrawlUrl::parse(&format!("{base}{}", paths[2])).unwrap();

    let fetched = f.fetch(&url).await.unwrap();
    let record = PageRecord::from_fetched(&fetched, 0, vec![]);

    assert_eq!(record.elapsed_ms, fetched.elapsed.as_millis() as u32);
    assert!(record.time_to_headers_ms <= record.elapsed_ms);

    server.abort();
}

// ---- meta robots ---------------------------------------------------------

#[test]
fn meta_robots_defaults_to_indexable() {
    let r = MetaRobots::default();
    assert!(r.is_indexable());
    assert!(!r.nofollow);
}

#[test]
fn meta_robots_parses_the_directives_that_matter() {
    let r = MetaRobots::parse("noindex, nofollow");
    assert!(r.noindex && r.nofollow);
    assert!(!r.is_indexable());

    let r = MetaRobots::parse("NOINDEX");
    assert!(r.noindex, "directives are case-insensitive");

    let r = MetaRobots::parse("  noarchive ,nosnippet ");
    assert!(r.noarchive && r.nosnippet);
    assert!(r.is_indexable(), "neither one blocks indexing");
}

#[test]
fn none_means_noindex_and_nofollow() {
    let r = MetaRobots::parse("none");
    assert!(r.noindex && r.nofollow);
}

#[test]
fn all_and_unknown_directives_leave_the_defaults_alone() {
    assert_eq!(MetaRobots::parse("all"), MetaRobots::default());
    assert_eq!(MetaRobots::parse("max-snippet:-1"), MetaRobots::default());
    assert_eq!(MetaRobots::parse(""), MetaRobots::default());
}

#[test]
fn a_later_permissive_directive_cannot_cancel_an_earlier_restriction() {
    // Real pages carry `noindex, all`. Reading that as indexable would be a
    // silent, high-consequence misreport.
    let r = MetaRobots::parse("noindex, all");
    assert!(r.noindex);
}

#[test]
fn merging_directive_sources_keeps_every_restriction() {
    // A page may carry <meta name="robots"> and <meta name="googlebot"> and an
    // X-Robots-Tag header. No source may loosen what another tightened.
    let meta = MetaRobots::parse("noindex");
    let header = MetaRobots::parse("nofollow");
    let merged = meta.or(header);
    assert!(merged.noindex && merged.nofollow);
    assert_eq!(merged, header.or(meta), "the merge is order-independent");
}

// ---- absent is not empty -------------------------------------------------

#[test]
fn an_absent_alt_is_distinguishable_from_an_empty_one() {
    let missing = Image {
        src: "/a.png".into(),
        alt: None,
    };
    let decorative = Image {
        src: "/b.png".into(),
        alt: Some(String::new()),
    };
    // One is a defect, the other is a deliberate decorative marker. A type
    // that merged them would make the rule unwritable.
    assert_ne!(missing.alt, decorative.alt);
}

// ---- serialisation -------------------------------------------------------

#[test]
fn a_record_survives_a_json_round_trip() {
    let record = PageRecord {
        url: CrawlUrl::parse("https://example.com/a").unwrap(),
        status: 200,
        depth: 1,
        size: 12,
        truncated: false,
        content_type: Some("text/html".into()),
        charset: Some("utf-8".into()),
        elapsed_ms: 5,
        time_to_headers_ms: 2,
        redirect_chain: vec!["https://example.com/old".into()],
        title: Some(String::new()),
        meta_description: None,
        h1: vec!["Heading".into()],
        h2: vec![],
        canonical: Some("/a".into()),
        canonical_url: Some(CrawlUrl::parse("https://example.com/a").unwrap()),
        meta_robots: MetaRobots::parse("noindex"),
        hreflang: vec![Hreflang {
            lang: "en-gb".into(),
            href: "/en/a".into(),
        }],
        open_graph: vec![("title".into(), "A".into())],
        links: vec![Link {
            href: "/b".into(),
            target: Some(CrawlUrl::parse("https://example.com/b").unwrap()),
            text: "B".into(),
            nofollow: true,
        }],
        images: vec![Image {
            src: "/i.png".into(),
            alt: None,
        }],
        word_count: 3,
    };

    let json = serde_json::to_string(&record).unwrap();
    let back: PageRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(record, back);
    // The empty title must not come back as absent.
    assert_eq!(back.title, Some(String::new()));
}

#[test]
fn a_url_that_never_passed_validation_cannot_be_deserialised() {
    // The whole value of CrawlUrl is that holding one proves the checks ran.
    // A hand-edited file must not be able to smuggle one past them.
    assert!(serde_json::from_str::<CrawlUrl>("\"ftp://example.com/x\"").is_err());
    assert!(serde_json::from_str::<CrawlUrl>("\"not a url\"").is_err());
    // A fragment is stripped on the way in rather than rejected.
    let u: CrawlUrl = serde_json::from_str("\"https://example.com/a#frag\"").unwrap();
    assert_eq!(u.to_string(), "https://example.com/a");
}
