//! The sitemap comparison, and when it refuses to make one.

use pounce_core::CrawlUrl;
use pounce_store::{SitemapFile, Store, Writer};

fn store_with(truncated: bool) -> Store {
    let mut store = Store::in_memory().unwrap();
    {
        let mut writer = Writer::new(&mut store);
        for path in ["a", "b", "c"] {
            let mut record = common_record(&format!("https://example.com/{path}"));
            record.word_count = 400;
            writer.push(&record).unwrap();
        }
        writer.flush().unwrap();
    }
    store
        .put_sitemap(&SitemapFile {
            url: "https://example.com/sitemap.xml".into(),
            status: 200,
            urls: 1,
            is_index: false,
            found_by: "robots".into(),
            truncated,
        })
        .unwrap();
    store
        .put_sitemap_urls(
            "https://example.com/sitemap.xml",
            &["https://example.com/a".to_string()],
        )
        .unwrap();
    store
}

fn common_record(url: &str) -> pounce_parse::PageRecord {
    pounce_parse::PageRecord {
        url: CrawlUrl::parse(url).unwrap(),
        status: 200,
        depth: 1,
        size: 1_000,
        truncated: false,
        content_type: Some("text/html".into()),
        charset: None,
        kind: pounce_parse::BodyKind::Html,
        content_type_mismatch: false,
        elapsed_ms: 5,
        time_to_headers_ms: 2,
        redirect_chain: vec![],
        title: Some("A title".into()),
        title_count: 1,
        meta_description: None,
        h1: vec![],
        h2: vec![],
        canonical: None,
        canonical_url: None,
        meta_robots: pounce_parse::MetaRobots::default(),
        hreflang: vec![],
        open_graph: vec![],
        links: vec![],
        images: vec![],
        word_count: 400,
        body_hash: None,
    }
}

#[test]
fn a_whole_sitemap_gives_the_comparison() {
    let summary = store_with(false).sitemap_summary().unwrap();
    assert_eq!(summary.urls, 1);
    // Three crawled pages, one of them listed.
    assert_eq!(summary.not_listed, Some(2));
}

#[test]
fn a_truncated_sitemap_refuses_the_comparison() {
    // Past the cap we do not know what the site listed, so every unmatched
    // page might be listed after all. Saying "2 pages missing" would be the
    // most confident possible way to be wrong — the number is invented by our
    // own limit rather than found on the site.
    let summary = store_with(true).sitemap_summary().unwrap();
    assert_eq!(summary.urls, 1, "what was read is still known");
    assert_eq!(
        summary.not_listed, None,
        "the comparison was made against a sitemap we only partly read"
    );
    // The other half still stands: a URL we *did* read and never reached is a
    // finding whether or not the file was cut short.
    assert_eq!(summary.not_crawled, 0);
}
