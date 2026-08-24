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
        title_count: 1,
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
        body_hash: Some(0x1234),
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

    // The repeating fields live in `page_detail` since migration 012, and must
    // come back as JSON rather than a Rust Debug rendering nothing can parse.
    let images: String = conn
        .query_row(
            "SELECT d.images FROM page_detail d JOIN pages p ON p.id = d.page_id",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let parsed: Vec<pounce_parse::Image> = serde_json::from_str(&images).unwrap();
    assert_eq!(parsed[0].src, "/i.png");
    assert_eq!(
        parsed[0].alt, None,
        "an absent alt stays absent through SQL"
    );
}

#[test]
fn a_re_fetched_page_updates_its_detail_rather_than_adding_one() {
    // The detail row is keyed by page id, so a second fetch of the same URL
    // must overwrite it. Without the upsert this is a constraint violation
    // mid-crawl; with `INSERT OR IGNORE` it would be last-crawl's headings
    // shown against this crawl's page.
    let mut store = Store::in_memory().unwrap();
    let mut first = record("https://example.com/a");
    first.h1 = vec!["Before".into()];
    let mut second = record("https://example.com/a");
    second.h1 = vec!["After".into()];
    {
        let mut writer = Writer::with_batch_size(&mut store, 8);
        writer.push(&first).unwrap();
        writer.push(&second).unwrap();
        writer.flush().unwrap();
    }

    let conn = store.conn();
    let rows: i64 = conn
        .query_row("SELECT count(*) FROM page_detail", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 1, "one detail row per page, not per fetch");
    let h1: String = conn
        .query_row("SELECT h1 FROM page_detail", [], |r| r.get(0))
        .unwrap();
    assert_eq!(h1, r#"["After"]"#);
}

#[test]
fn deleting_a_page_takes_its_detail_with_it() {
    // The same cascade the issues and links tables rely on. A detail row
    // outliving its page is a row the detail pane can never reach again.
    let mut store = Store::in_memory().unwrap();
    {
        let mut writer = Writer::with_batch_size(&mut store, 1);
        writer.push(&record("https://example.com/a")).unwrap();
        writer.flush().unwrap();
    }
    store.conn().execute("DELETE FROM pages", []).unwrap();
    let rows: i64 = store
        .conn()
        .query_row("SELECT count(*) FROM page_detail", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 0);
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

// ---- issues (T2.3) ------------------------------------------------------

#[test]
fn issues_commit_in_the_same_transaction_as_their_page() {
    // There must be no state where a page exists with half its findings.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c.pounce");
    {
        let mut store = Store::open(&path).unwrap();
        let mut writer = Writer::with_batch_size(&mut store, 10);
        let id = writer.push(&record("https://example.com/a")).unwrap();
        writer
            .issues(
                "https://example.com/a",
                Some(id),
                &[("title.missing", "critical", None)],
            )
            .unwrap();
        // Dropped without flush: the shape of a crawl that was killed.
    }
    let reopened = Store::open(&path).unwrap();
    let pages: i64 = reopened
        .conn()
        .query_row("SELECT count(*) FROM pages", [], |r| r.get(0))
        .unwrap();
    let issues: i64 = reopened
        .conn()
        .query_row("SELECT count(*) FROM issues", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        (pages, issues),
        (0, 0),
        "page and issues roll back together"
    );
}

#[test]
fn an_issue_stores_its_rule_severity_and_detail() {
    let mut store = Store::in_memory().unwrap();
    {
        let mut writer = Writer::with_batch_size(&mut store, 4);
        let id = writer.push(&record("https://example.com/a")).unwrap();
        writer
            .issues(
                "https://example.com/a",
                Some(id),
                &[
                    ("title.too-long", "warning", Some("84 characters")),
                    ("media.missing-alt", "warning", None),
                ],
            )
            .unwrap();
        writer.flush().unwrap();
    }
    let rows: Vec<(String, String, Option<String>)> = store
        .conn()
        .prepare("SELECT rule_id, severity, detail FROM issues ORDER BY rule_id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        rows[0],
        ("media.missing-alt".into(), "warning".into(), None)
    );
    assert_eq!(
        rows[1],
        (
            "title.too-long".into(),
            "warning".into(),
            Some("84 characters".into())
        )
    );
}

#[test]
fn writing_no_issues_is_not_an_error_and_opens_no_transaction() {
    let mut store = Store::in_memory().unwrap();
    let mut writer = Writer::with_batch_size(&mut store, 4);
    let id = writer.push(&record("https://example.com/a")).unwrap();
    writer
        .issues("https://example.com/a", Some(id), &[])
        .unwrap();
    writer.flush().unwrap();
}

#[test]
fn push_returns_the_same_id_when_a_url_is_re_fetched() {
    // Resume re-fetches. If the id changed, every issue written against the
    // old one would be destroyed by the ON DELETE CASCADE.
    let mut store = Store::in_memory().unwrap();
    let mut writer = Writer::with_batch_size(&mut store, 4);
    let first = writer.push(&record("https://example.com/a")).unwrap();
    let second = writer.push(&record("https://example.com/a")).unwrap();
    assert_eq!(first, second);
    writer.flush().unwrap();
}

#[test]
fn distinct_urls_get_distinct_ids() {
    let mut store = Store::in_memory().unwrap();
    let mut writer = Writer::with_batch_size(&mut store, 4);
    let a = writer.push(&record("https://example.com/a")).unwrap();
    let b = writer.push(&record("https://example.com/b")).unwrap();
    assert_ne!(a, b);
    writer.flush().unwrap();
}

// ---- page content columns (T2.0c) --------------------------------------

#[test]
fn title_count_and_body_hash_survive_the_write() {
    // `duplicate body` is a SiteRule and reads SQL, so an unpersisted
    // body_hash makes the rule unwritable.
    let mut store = Store::in_memory().unwrap();
    let mut with = record("https://example.com/a");
    with.title_count = 3;
    with.body_hash = Some(0x8594_4171_f739_67e8);
    {
        let mut writer = Writer::with_batch_size(&mut store, 4);
        writer.push(&with).unwrap();
        writer.flush().unwrap();
    }
    let (count, hash): (i64, Option<i64>) = store
        .conn()
        .query_row("SELECT title_count, body_hash FROM pages", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(count, 3);
    // SQLite integers are signed; the u64 round-trips through the same
    // reinterpretation on the way out, so equality is what matters.
    assert_eq!(hash, Some(0x8594_4171_f739_67e8_u64 as i64));
}

#[test]
fn a_page_with_no_body_text_stores_a_null_hash() {
    // NULL means "nothing to compare". Storing 0 would make every blank page
    // a duplicate of every other blank page.
    let mut store = Store::in_memory().unwrap();
    let mut empty = record("https://example.com/empty");
    empty.body_hash = None;
    {
        let mut writer = Writer::with_batch_size(&mut store, 4);
        writer.push(&empty).unwrap();
        writer.flush().unwrap();
    }
    let nulls: i64 = store
        .conn()
        .query_row(
            "SELECT count(*) FROM pages WHERE body_hash IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(nulls, 1);
}

#[test]
fn an_issue_can_be_about_a_url_that_never_became_a_page() {
    // A redirect loop never produces a page: the source lands in
    // crawl_redirects and crawl_failures, not in `pages`. The finding still has
    // to be recordable, or --fail-on would pass a site full of loops.
    let mut store = Store::in_memory().unwrap();
    {
        let mut writer = Writer::with_batch_size(&mut store, 4);
        writer
            .issues(
                "https://example.com/loop",
                None,
                &[("response.redirect-loop", "critical", Some("3 hops"))],
            )
            .unwrap();
        writer.flush().unwrap();
    }
    let (url, page_id): (String, Option<i64>) = store
        .conn()
        .query_row("SELECT url, page_id FROM issues", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(url, "https://example.com/loop");
    assert_eq!(page_id, None, "there is no page to point at");
}

#[test]
fn an_issue_about_a_page_still_carries_both_url_and_page_id() {
    let mut store = Store::in_memory().unwrap();
    let url = "https://example.com/a";
    {
        let mut writer = Writer::with_batch_size(&mut store, 4);
        let id = writer.push(&record(url)).unwrap();
        writer
            .issues(url, Some(id), &[("title.missing", "critical", None)])
            .unwrap();
        writer.flush().unwrap();
    }
    let (stored_url, page_id): (String, Option<i64>) = store
        .conn()
        .query_row("SELECT url, page_id FROM issues", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(stored_url, url);
    assert!(page_id.is_some(), "the join to pages must still be there");
}

#[test]
fn deleting_a_page_clears_its_issues_but_not_the_pageless_ones() {
    // The cascade must not take orphan-subject findings with it: a loop's
    // issue is not about any page, so no page deletion should remove it.
    let mut store = Store::in_memory().unwrap();
    {
        let mut writer = Writer::with_batch_size(&mut store, 8);
        let id = writer.push(&record("https://example.com/a")).unwrap();
        writer
            .issues(
                "https://example.com/a",
                Some(id),
                &[("title.missing", "critical", None)],
            )
            .unwrap();
        writer
            .issues(
                "https://example.com/loop",
                None,
                &[("response.redirect-loop", "critical", None)],
            )
            .unwrap();
        writer.flush().unwrap();
    }
    store.conn().execute("DELETE FROM pages", []).unwrap();
    let left: Vec<String> = store
        .conn()
        .prepare("SELECT rule_id FROM issues")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(left, ["response.redirect-loop"]);
}

// ---- resources ----------------------------------------------------------

fn resources(store: &Store) -> Vec<(String, i64, Option<i64>, Option<String>)> {
    store
        .conn()
        .prepare("SELECT url, status, content_length, content_type FROM resources ORDER BY url")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn a_resource_records_status_length_and_type() {
    let mut store = Store::in_memory().unwrap();
    let url = CrawlUrl::parse("https://example.com/logo.png").unwrap();
    {
        let mut writer = Writer::with_batch_size(&mut store, 1);
        writer
            .resource(&url, 200, Some(4096), Some("image/png"))
            .unwrap();
        writer.flush().unwrap();
    }
    assert_eq!(
        resources(&store),
        [(
            "https://example.com/logo.png".to_string(),
            200,
            Some(4096),
            Some("image/png".to_string())
        )]
    );
}

#[test]
fn an_undeclared_length_stays_null_rather_than_zero() {
    // NULL is "the server declared nothing" and 0 is "the server declared zero
    // bytes". `media.oversized-image` has to read the first as unknown, so
    // collapsing them here would make the rule quietly answer the wrong
    // question.
    let mut store = Store::in_memory().unwrap();
    let url = CrawlUrl::parse("https://example.com/stream.svg").unwrap();
    {
        let mut writer = Writer::with_batch_size(&mut store, 1);
        writer
            .resource(&url, 200, None, Some("image/svg+xml"))
            .unwrap();
        writer.flush().unwrap();
    }
    assert_eq!(resources(&store)[0].2, None);
}

#[test]
fn the_same_resource_seen_twice_is_one_row() {
    // A logo in a template is discovered once per page that uses it. Without
    // the upsert, a 500k crawl would write 500k rows for one image.
    let mut store = Store::in_memory().unwrap();
    let url = CrawlUrl::parse("https://example.com/logo.png").unwrap();
    {
        let mut writer = Writer::with_batch_size(&mut store, 2);
        writer.resource(&url, 500, None, None).unwrap();
        writer
            .resource(&url, 200, Some(10), Some("image/png"))
            .unwrap();
        writer.flush().unwrap();
    }
    let rows = resources(&store);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1, 200, "the later check wins");
    assert_eq!(rows[0].2, Some(10));
}

#[test]
fn a_declared_length_past_four_gigabytes_round_trips() {
    // The column is a signed INTEGER and the header parses as u64. A cast that
    // wrapped would report a 5 GB download as a small file — the exact case
    // `media.oversized-image` exists to catch.
    let mut store = Store::in_memory().unwrap();
    let url = CrawlUrl::parse("https://example.com/huge.tif").unwrap();
    let big: u64 = 5_000_000_000;
    {
        let mut writer = Writer::with_batch_size(&mut store, 1);
        writer
            .resource(&url, 200, Some(big), Some("image/tiff"))
            .unwrap();
        writer.flush().unwrap();
    }
    assert_eq!(resources(&store)[0].2, Some(big as i64));
}
