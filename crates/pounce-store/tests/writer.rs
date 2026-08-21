//! The batched writer: what is durable when, and what a re-fetch does.

use pounce_core::CrawlUrl;
use pounce_parse::{BodyKind, Image, Link, MetaRobots, PageRecord};
use pounce_store::{Store, Writer};
use rusqlite::Connection;

fn record(url: &str) -> PageRecord {
    PageRecord {
        url: CrawlUrl::parse(url).unwrap(),
        status: 200,
        depth: 1,
        size: 1234,
        truncated: false,
        content_type: Some("text/html".into()),
        charset: Some("utf-8".into()),
        kind: BodyKind::Html,
        content_type_mismatch: false,
        elapsed_ms: 12,
        time_to_headers_ms: 4,
        redirect_chain: vec![],
        title: Some("A page".into()),
        meta_description: None,
        h1: vec!["Heading".into()],
        h2: vec![],
        canonical: Some("/a".into()),
        canonical_url: Some(CrawlUrl::parse("https://example.com/a").unwrap()),
        meta_robots: MetaRobots::parse("noindex"),
        hreflang: vec![],
        open_graph: vec![("title".into(), "A".into())],
        links: vec![Link {
            href: "/b".into(),
            target: Some(CrawlUrl::parse("https://example.com/b").unwrap()),
            text: "B".into(),
            nofollow: false,
        }],
        images: vec![Image {
            src: "/i.png".into(),
            alt: None,
        }],
        word_count: 42,
    }
}

fn count(conn: &Connection) -> i64 {
    conn.query_row("SELECT count(*) FROM pages", [], |r| r.get(0))
        .unwrap()
}

// ---- batching ------------------------------------------------------------

#[test]
fn rows_are_not_visible_until_the_batch_commits() {
    let mut store = Store::in_memory().unwrap();
    let mut writer = Writer::with_batch_size(&mut store, 5);

    for i in 0..4 {
        writer
            .push(&record(&format!("https://example.com/{i}")))
            .unwrap();
    }
    // Four in an open transaction. Nothing has been committed, so `committed`
    // must not claim otherwise — a progress counter that runs ahead of durable
    // state is how a resumed crawl loses rows.
    assert_eq!(writer.committed(), 0);

    let flushed = writer.flush().unwrap();
    assert_eq!(flushed, 4);
    assert_eq!(writer.committed(), 4);
}

#[test]
fn a_full_batch_commits_on_its_own() {
    let mut store = Store::in_memory().unwrap();
    {
        let mut writer = Writer::with_batch_size(&mut store, 3);
        for i in 0..7 {
            writer
                .push(&record(&format!("https://example.com/{i}")))
                .unwrap();
        }
        // Two full batches of three committed; the seventh row is still open.
        assert_eq!(writer.committed(), 6);

        writer.flush().unwrap();
        assert_eq!(writer.committed(), 7);
    }
    assert_eq!(count(store.conn()), 7);
}

#[test]
fn flushing_an_empty_writer_is_not_an_error() {
    let mut store = Store::in_memory().unwrap();
    let mut writer = Writer::with_batch_size(&mut store, 5);
    assert_eq!(writer.flush().unwrap(), 0);
    assert_eq!(writer.flush().unwrap(), 0);
    assert_eq!(writer.committed(), 0);
}

#[test]
fn the_default_batch_size_is_the_documented_one() {
    assert_eq!(pounce_store::BATCH_SIZE, 500);
}

// ---- round trip ----------------------------------------------------------

#[test]
fn every_field_survives_the_write() {
    let mut store = Store::in_memory().unwrap();
    let original = record("https://example.com/a");
    {
        let mut writer = Writer::with_batch_size(&mut store, 1);
        writer.push(&original).unwrap();
        writer.flush().unwrap();
    }

    let conn = store.conn();
    let (url, status, title, word_count, noindex, kind): (
        String,
        i64,
        Option<String>,
        i64,
        i64,
        String,
    ) = conn
        .query_row(
            "SELECT url, status, title, word_count, noindex, kind FROM pages",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .unwrap();

    assert_eq!(url, "https://example.com/a");
    assert_eq!(status, 200);
    assert_eq!(title.as_deref(), Some("A page"));
    assert_eq!(word_count, 42);
    assert_eq!(noindex, 1, "meta robots is stored as queryable columns");
    assert_eq!(kind, "html");

    // The repeating fields are JSON, and must come back as JSON rather than a
    // Rust Debug rendering that nothing can parse.
    let images: String = conn
        .query_row("SELECT images FROM pages", [], |r| r.get(0))
        .unwrap();
    let parsed: Vec<pounce_parse::Image> = serde_json::from_str(&images).unwrap();
    assert_eq!(parsed[0].src, "/i.png");
    assert_eq!(
        parsed[0].alt, None,
        "an absent alt stays absent through SQL"
    );
}

#[test]
fn an_absent_title_is_stored_as_null_not_as_an_empty_string() {
    // SQL sorting and "missing title" rules both depend on this. An empty
    // title is a finding; a missing one is a different finding.
    let mut store = Store::in_memory().unwrap();
    let mut empty = record("https://example.com/empty");
    empty.title = Some(String::new());
    let mut absent = record("https://example.com/absent");
    absent.title = None;
    {
        let mut writer = Writer::with_batch_size(&mut store, 8);
        writer.push(&empty).unwrap();
        writer.push(&absent).unwrap();
        writer.flush().unwrap();
    }

    let nulls: i64 = store
        .conn()
        .query_row("SELECT count(*) FROM pages WHERE title IS NULL", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(nulls, 1);
    let empties: i64 = store
        .conn()
        .query_row("SELECT count(*) FROM pages WHERE title = ''", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(empties, 1);
}

// ---- link graph ----------------------------------------------------------

#[test]
fn links_survive_the_write_with_raw_and_resolved_values() {
    let mut store = Store::in_memory().unwrap();
    {
        let mut writer = Writer::with_batch_size(&mut store, 1);
        writer.push(&record("https://example.com/a")).unwrap();
        writer.flush().unwrap();
    }

    let link: (String, Option<String>, String, i64) = store
        .conn()
        .query_row(
            "SELECT href, target_url, anchor_text, nofollow FROM links",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        link,
        (
            "/b".into(),
            Some("https://example.com/b".into()),
            "B".into(),
            0,
        )
    );
}

#[test]
fn re_fetching_a_page_replaces_its_outlinks() {
    let mut store = Store::in_memory().unwrap();
    let first = record("https://example.com/a");
    let mut second = record("https://example.com/a");
    second.links = vec![Link {
        href: "mailto:hello@example.com".into(),
        target: None,
        text: "Email".into(),
        nofollow: true,
    }];

    {
        let mut writer = Writer::with_batch_size(&mut store, 1);
        writer.push(&first).unwrap();
        writer.push(&second).unwrap();
        writer.flush().unwrap();
    }

    let links: Vec<(String, Option<String>, i64)> = store
        .conn()
        .prepare("SELECT href, target_url, nofollow FROM links")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(links, vec![("mailto:hello@example.com".into(), None, 1)]);
}

// ---- re-fetching the same URL -------------------------------------------

#[test]
fn re_fetching_a_url_updates_the_row_rather_than_adding_one() {
    let mut store = Store::in_memory().unwrap();
    let mut first = record("https://example.com/a");
    first.status = 200;
    let mut second = record("https://example.com/a");
    second.status = 404;
    second.title = None;

    {
        let mut writer = Writer::with_batch_size(&mut store, 1);
        writer.push(&first).unwrap();
        writer.push(&second).unwrap();
        writer.flush().unwrap();
    }

    assert_eq!(count(store.conn()), 1, "a resumed crawl must not duplicate");
    let (status, title): (i64, Option<String>) = store
        .conn()
        .query_row("SELECT status, title FROM pages", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(status, 404, "the newer fetch wins");
    assert_eq!(title, None, "and it clears fields it no longer has");
}

#[test]
fn an_updated_row_keeps_its_id() {
    // INSERT OR REPLACE would delete and re-insert, changing the id and
    // orphaning every link edge pointing at this page.
    let mut store = Store::in_memory().unwrap();
    {
        let mut writer = Writer::with_batch_size(&mut store, 1);
        writer.push(&record("https://example.com/a")).unwrap();
        writer.flush().unwrap();
    }
    let before: i64 = store
        .conn()
        .query_row("SELECT id FROM pages", [], |r| r.get(0))
        .unwrap();

    {
        let mut writer = Writer::with_batch_size(&mut store, 1);
        let mut again = record("https://example.com/a");
        again.status = 500;
        writer.push(&again).unwrap();
        writer.flush().unwrap();
    }
    let after: i64 = store
        .conn()
        .query_row("SELECT id FROM pages", [], |r| r.get(0))
        .unwrap();
    assert_eq!(before, after);
}

// ---- durability ----------------------------------------------------------

#[test]
fn a_committed_batch_survives_dropping_the_writer_and_reopening_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crawl.pounce");
    {
        let mut store = Store::open(&path).unwrap();
        let mut writer = Writer::with_batch_size(&mut store, 2);
        for i in 0..5 {
            writer
                .push(&record(&format!("https://example.com/{i}")))
                .unwrap();
        }
        // Four committed, one left open, and the writer is dropped without a
        // flush — the shape of a crawl that was killed.
        assert_eq!(writer.committed(), 4);
    }

    let reopened = Store::open(&path).unwrap();
    assert_eq!(
        count(reopened.conn()),
        4,
        "committed batches survive; the open one is lost, which is why the \
         frontier and not the page table decides what to re-fetch"
    );
}
