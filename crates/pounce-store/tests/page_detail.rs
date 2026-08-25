//! One page, in full — the other end of the design from `query_rows`.

mod common;

use pounce_core::CrawlUrl;
use pounce_parse::{Image, Link, PageRecord};
use pounce_store::{MAX_LINKS, Store, Writer};

/// A crawl of three pages: a hub linking to both others, and one of those
/// linking back. Enough to tell inlinks from outlinks, which is the pair a
/// single-page fixture cannot distinguish.
fn seeded() -> Store {
    let mut store = Store::in_memory().unwrap();
    let mut hub = common::record_for("http://e.com/hub");
    hub.title = Some("The hub".into());
    hub.h1 = vec!["Hub".into()];
    hub.images = vec![Image {
        src: "http://e.com/a.png".into(),
        alt: None,
    }];
    hub.redirect_chain = vec!["http://e.com/old-hub".into()];
    hub.links = vec![
        Link {
            href: "/a".into(),
            target: Some(CrawlUrl::parse("http://e.com/a").unwrap()),
            text: "To A".into(),
            nofollow: false,
        },
        Link {
            href: "mailto:x@e.com".into(),
            target: None,
            text: "Mail".into(),
            nofollow: false,
        },
    ];

    let mut a = common::record_for("http://e.com/a");
    a.links = vec![Link {
        href: "/hub".into(),
        target: Some(CrawlUrl::parse("http://e.com/hub").unwrap()),
        text: "Back to the hub".into(),
        nofollow: true,
    }];

    {
        let mut writer = Writer::with_batch_size(&mut store, 8);
        writer.push(&hub).unwrap();
        writer.push(&a).unwrap();
        writer.flush().unwrap();
    }
    store
}

fn id_of(store: &Store, url: &str) -> i64 {
    store
        .conn()
        .query_row("SELECT id FROM pages WHERE url = ?1", [url], |r| r.get(0))
        .unwrap()
}

#[test]
fn a_page_comes_back_whole() {
    let store = seeded();
    let detail = store
        .page_detail(id_of(&store, "http://e.com/hub"))
        .unwrap()
        .expect("the hub is a page in this crawl");

    assert_eq!(detail.url, "http://e.com/hub");
    assert_eq!(detail.title.as_deref(), Some("The hub"));
    // The JSON columns cross as JSON, not as strings holding JSON: the UI
    // reading `h1[0]` should not have to parse anything.
    assert_eq!(detail.h1, serde_json::json!(["Hub"]));
    assert_eq!(
        detail.redirect_chain,
        serde_json::json!(["http://e.com/old-hub"])
    );
    // Absent is not empty, all the way out to the pane.
    assert_eq!(detail.images[0]["alt"], serde_json::Value::Null);
}

#[test]
fn inlinks_and_outlinks_point_in_opposite_directions() {
    let store = seeded();
    let hub = store
        .page_detail(id_of(&store, "http://e.com/hub"))
        .unwrap()
        .unwrap();

    // Out: two anchors, one of which leaves for a `mailto:` and so was never
    // crawled — shown as written rather than dropped.
    assert_eq!(hub.outlink_count, 2);
    assert_eq!(hub.outlinks[0].url, "http://e.com/a");
    assert!(hub.outlinks[0].crawled);
    assert_eq!(hub.outlinks[1].url, "mailto:x@e.com");
    assert!(!hub.outlinks[1].crawled);

    // In: one, from the page the hub links to, and it carries the `nofollow`
    // that changes what the link is worth.
    assert_eq!(hub.inlink_count, 1);
    assert_eq!(hub.inlinks[0].url, "http://e.com/a");
    assert_eq!(hub.inlinks[0].anchor_text, "Back to the hub");
    assert!(hub.inlinks[0].nofollow);
}

#[test]
fn a_missing_page_is_none_rather_than_an_error() {
    // The pane asks by id, and a file can be closed and reopened between the
    // click and the query. "Gone" is an answer; a failed query is not.
    let store = seeded();
    assert!(store.page_detail(9_999).unwrap().is_none());
}

#[test]
fn the_link_lists_are_capped_and_the_counts_are_not() {
    // A hub page on a real site has tens of thousands of inlinks. Sending them
    // all would break the invariant this whole crate exists to protect, so the
    // list is a sample and the count is the truth.
    let mut store = Store::in_memory().unwrap();
    let target = "http://e.com/popular";
    let mut popular = common::record_for(target);
    popular.links = Vec::new();

    {
        let mut writer = Writer::with_batch_size(&mut store, 64);
        writer.push(&popular).unwrap();
        for i in 0..(MAX_LINKS as u64 + 25) {
            let mut linker: PageRecord = common::record_for(&format!("http://e.com/p{i}"));
            linker.links = vec![Link {
                href: target.into(),
                target: Some(CrawlUrl::parse(target).unwrap()),
                text: format!("link {i}"),
                nofollow: false,
            }];
            writer.push(&linker).unwrap();
        }
        writer.flush().unwrap();
    }

    let detail = store.page_detail(id_of(&store, target)).unwrap().unwrap();
    assert_eq!(detail.inlinks.len(), MAX_LINKS);
    assert_eq!(detail.inlink_count, MAX_LINKS as u64 + 25);
}

/// Times the two queries the results screen runs against a store you point it
/// at: the issue overview, which the findings rail redraws once a second while
/// a crawl writes, and `page_detail`, which runs on every row a user opens.
///
/// ```
/// DETAIL_IN=/tmp/bench-run.pounce cargo test --release -p pounce-store \
///   --test page_detail time_the_results_screen -- --ignored --nocapture
/// ```
#[test]
#[ignore = "needs a crawled store; run with --release --ignored --nocapture"]
fn time_the_results_screen() {
    let path = std::env::var("DETAIL_IN").expect("set DETAIL_IN to a .pounce file");
    let store = Store::open_read_only(&path).unwrap();

    let started = std::time::Instant::now();
    let overview = store.issue_overview().unwrap();
    let overview_ms = started.elapsed();

    // The worst case for the pane is the page the most links point at: the
    // inlink count is not capped, and a hub is what makes it expensive.
    // A store with no links at all is a legitimate thing to point this at — a
    // seeded fixture, for instance — so fall back to any page rather than
    // failing on a file that simply has no hub.
    let hub = store
        .conn()
        .query_row(
            "SELECT p.id, p.url, count(*) c FROM links l JOIN pages p ON p.url = l.target_url \
             GROUP BY l.target_url ORDER BY c DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .ok();
    let (hub_id, hub_url, inlinks): (i64, String, i64) = match hub {
        Some(hub) => hub,
        None => store
            .conn()
            .query_row("SELECT id, url, 0 FROM pages LIMIT 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap(),
    };

    let started = std::time::Instant::now();
    let detail = store.page_detail(hub_id).unwrap().unwrap();
    let detail_ms = started.elapsed();

    eprintln!(
        "{path}\n  issue_overview: {overview_ms:?} ({} findings, {} rules)\n  \
         page_detail on the busiest page ({hub_url}, {inlinks} inlinks): {detail_ms:?} \
         (returned {} inlinks, {} outlinks, {} findings)",
        overview.total_issues,
        overview.by_rule.len(),
        detail.inlinks.len(),
        detail.outlinks.len(),
        detail.issues.len(),
    );
}
