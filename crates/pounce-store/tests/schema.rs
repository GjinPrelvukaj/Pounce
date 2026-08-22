//! Opening, migrating, and reopening a crawl database.

use pounce_store::schema::SCHEMA_VERSION;
use pounce_store::{Store, StoreError};
use rusqlite::Connection;

fn scalar<T: rusqlite::types::FromSql>(conn: &Connection, sql: &str) -> T {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

/// Rewinds a current-schema file so it looks like one written at `version`.
///
/// Each migration after `version` is undone, newest first. Kept in one place
/// because the alternative — every test hand-rolling its own undo — means each
/// new migration silently breaks five tests at once, which is exactly what
/// migration 008 did.
fn rewind_to(conn: &Connection, version: u32) {
    // (version introduced, how to undo it), applied newest first. 007 is
    // absent because it only drops an index, which replays harmlessly. 005 is
    // undone by rebuilding `crawl` at its 003 shape rather than by DROP
    // COLUMN, which SQLite refuses for a column named in a CHECK constraint.
    let undo: &[(u32, &str)] = &[
        (11, "DROP TABLE resources;"),
        (
            9,
            "ALTER TABLE pages DROP COLUMN body_hash; \
             ALTER TABLE pages DROP COLUMN title_count;",
        ),
        (8, "DROP TABLE issues;"),
        (6, "DROP TABLE crawl_redirects;"),
        (
            5,
            "DROP TABLE crawl; \
             CREATE TABLE crawl ( \
                 id       INTEGER PRIMARY KEY CHECK (id = 1), \
                 seed_url TEXT NOT NULL \
             ) STRICT;",
        ),
        (4, "DROP TABLE crawl_failures;"),
        (3, "DROP TABLE frontier; DROP TABLE crawl;"),
    ];
    for (introduced, sql) in undo {
        if *introduced > version {
            conn.execute_batch(sql).unwrap();
        }
    }
    conn.pragma_update(None, "user_version", version).unwrap();
}

fn table_exists(conn: &Connection, name: &str) -> bool {
    scalar::<i64>(
        conn,
        &format!("SELECT count(*) FROM sqlite_master WHERE type='table' AND name='{name}'"),
    ) == 1
}

// ---- pragmas -------------------------------------------------------------

#[test]
fn a_file_database_runs_in_wal_mode() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("crawl.pounce")).unwrap();

    // WAL is what lets the UI read while the writer commits. Without it the
    // app freezes mid-crawl every time a batch lands, which is precisely the
    // "feels slower than Electron" outcome the architecture exists to avoid.
    let mode: String = scalar(store.conn(), "PRAGMA journal_mode");
    assert_eq!(mode.to_lowercase(), "wal");
}

#[test]
fn foreign_keys_are_enforced() {
    // Off by default in SQLite, and a link row pointing at a deleted page is
    // exactly the corruption that produces a report with phantom inlinks.
    let store = Store::in_memory().unwrap();
    assert_eq!(scalar::<i64>(store.conn(), "PRAGMA foreign_keys"), 1);
}

// ---- migrations ----------------------------------------------------------

#[test]
fn a_new_database_is_at_the_current_version() {
    let store = Store::in_memory().unwrap();
    assert_eq!(store.version().unwrap(), SCHEMA_VERSION);
    const { assert!(SCHEMA_VERSION >= 1) };
}

#[test]
fn reopening_does_not_re_run_migrations() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crawl.pounce");

    let first = Store::open(&path).unwrap();
    first
        .conn()
        .execute("INSERT INTO pages (url, status, depth, size, truncated, kind, content_type_mismatch, elapsed_ms, time_to_headers_ms, redirect_chain, h1, h2, noindex, nofollow, noarchive, nosnippet, hreflang, open_graph, images, word_count) VALUES ('https://a/', 200, 0, 1, 0, 'html', 0, 1, 1, '[]', '[]', '[]', 0, 0, 0, 0, '[]', '[]', '[]', 0)", [])
        .unwrap();
    drop(first);

    // A migration re-run would either error on CREATE TABLE or drop the row.
    let second = Store::open(&path).unwrap();
    assert_eq!(second.version().unwrap(), SCHEMA_VERSION);
    assert_eq!(
        scalar::<i64>(second.conn(), "SELECT count(*) FROM pages"),
        1
    );
}

#[test]
fn a_file_from_a_newer_build_is_refused_rather_than_corrupted() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crawl.pounce");
    Store::open(&path).unwrap();

    let conn = Connection::open(&path).unwrap();
    conn.pragma_update(None, "user_version", SCHEMA_VERSION + 5)
        .unwrap();
    drop(conn);

    // Opening it read-write and running nothing would look like it worked and
    // then write rows the newer schema cannot read back.
    match Store::open(&path) {
        Err(StoreError::TooNew { found, known }) => {
            assert_eq!(found, SCHEMA_VERSION + 5);
            assert_eq!(known, SCHEMA_VERSION);
        }
        other => panic!("expected TooNew, got {:?}", other.map(|_| "Ok")),
    }
}

#[test]
fn a_schema_two_file_gains_resume_state_without_losing_pages() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.pounce");
    {
        let store = Store::open(&path).unwrap();
        store
            .conn()
            .execute("INSERT INTO pages (url, status, depth, size, truncated, kind, content_type_mismatch, elapsed_ms, time_to_headers_ms, redirect_chain, h1, h2, noindex, nofollow, noarchive, nosnippet, hreflang, open_graph, images, word_count) VALUES ('https://a/', 200, 0, 1, 0, 'html', 0, 1, 1, '[]', '[]', '[]', 0, 0, 0, 0, '[]', '[]', '[]', 0)", [])
            .unwrap();
        rewind_to(store.conn(), 2);
    }

    let upgraded = Store::open(&path).unwrap();
    assert_eq!(upgraded.version().unwrap(), SCHEMA_VERSION);
    assert!(table_exists(upgraded.conn(), "crawl"));
    assert!(table_exists(upgraded.conn(), "frontier"));
    assert_eq!(
        scalar::<i64>(upgraded.conn(), "SELECT count(*) FROM pages"),
        1
    );
}

#[test]
fn terminal_fetch_failures_have_a_durable_table() {
    let store = Store::in_memory().unwrap();
    assert!(table_exists(store.conn(), "crawl_failures"));
}

#[test]
fn a_schema_three_file_gains_terminal_outcomes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("schema-three.pounce");
    {
        let store = Store::open(&path).unwrap();
        rewind_to(store.conn(), 3);
    }

    let upgraded = Store::open(&path).unwrap();
    assert!(table_exists(upgraded.conn(), "crawl_failures"));
    assert_eq!(upgraded.version().unwrap(), SCHEMA_VERSION);
}

#[test]
fn a_schema_four_file_gains_crawl_limits() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("schema-four.pounce");
    {
        let store = Store::open(&path).unwrap();
        rewind_to(store.conn(), 4);
    }

    let upgraded = Store::open(&path).unwrap();
    assert!(
        upgraded
            .conn()
            .prepare("SELECT max_depth, max_urls, max_duration_ns FROM crawl")
            .is_ok()
    );
    assert_eq!(upgraded.version().unwrap(), SCHEMA_VERSION);
}

#[test]
fn a_schema_ten_file_gains_the_resources_table() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("schema-ten.pounce");
    {
        let store = Store::open(&path).unwrap();
        rewind_to(store.conn(), 10);
    }

    let upgraded = Store::open(&path).unwrap();
    assert!(table_exists(upgraded.conn(), "resources"));
    assert_eq!(upgraded.version().unwrap(), SCHEMA_VERSION);
}

#[test]
fn a_schema_five_file_gains_redirect_outcomes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("schema-five.pounce");
    {
        let store = Store::open(&path).unwrap();
        rewind_to(store.conn(), 5);
    }

    let upgraded = Store::open(&path).unwrap();
    assert!(table_exists(upgraded.conn(), "crawl_redirects"));
    assert_eq!(upgraded.version().unwrap(), SCHEMA_VERSION);
}

// ---- the table -----------------------------------------------------------

#[test]
fn the_pages_table_exists_and_is_strict() {
    let store = Store::in_memory().unwrap();
    assert!(table_exists(store.conn(), "pages"));

    // STRICT tables reject a type mismatch instead of silently coercing it.
    // A status stored as the text "200" sorts as text, and the grid's ORDER BY
    // would then put 99 after 100.
    let err = store.conn().execute(
        "INSERT INTO pages (url, status, depth, size, truncated, kind, content_type_mismatch, elapsed_ms, time_to_headers_ms, redirect_chain, h1, h2, noindex, nofollow, noarchive, nosnippet, hreflang, open_graph, images, word_count) VALUES ('https://a/', 'not a number', 0, 1, 0, 'html', 0, 1, 1, '[]', '[]', '[]', 0, 0, 0, 0, '[]', '[]', '[]', 0)",
        [],
    );
    assert!(err.is_err(), "STRICT should reject a text status");
}

#[test]
fn a_url_cannot_be_stored_twice() {
    // The frontier dedupes, but the unique index is what makes a resumed crawl
    // safe: re-fetching a URL after a restart must not double the row count.
    let store = Store::in_memory().unwrap();
    let insert = "INSERT INTO pages (url, status, depth, size, truncated, kind, content_type_mismatch, elapsed_ms, time_to_headers_ms, redirect_chain, h1, h2, noindex, nofollow, noarchive, nosnippet, hreflang, open_graph, images, word_count) VALUES ('https://a/', 200, 0, 1, 0, 'html', 0, 1, 1, '[]', '[]', '[]', 0, 0, 0, 0, '[]', '[]', '[]', 0)";
    store.conn().execute(insert, []).unwrap();
    assert!(store.conn().execute(insert, []).is_err());
}

#[test]
fn every_sortable_column_is_indexed() {
    let store = Store::in_memory().unwrap();
    // Scrolling a sorted grid maps to OFFSET, so an unindexed sort column is a
    // full scan per screenful on a million-row table.
    for column in [
        "status",
        "depth",
        "size",
        "word_count",
        "elapsed_ms",
        "kind",
        "title",
        "noindex",
    ] {
        let plan: String = store
            .conn()
            .query_row(
                &format!("EXPLAIN QUERY PLAN SELECT id FROM pages ORDER BY {column} LIMIT 200"),
                [],
                |r| r.get(3),
            )
            .unwrap();
        assert!(
            plan.contains("USING INDEX") || plan.contains("USING COVERING INDEX"),
            "ORDER BY {column} did not use an index: {plan}"
        );
    }
}

#[test]
fn url_lookup_is_indexed_too() {
    // Every discovered link asks "have we seen this?" — the hottest query in a
    // crawl, and a scan here would make the frontier quadratic.
    let store = Store::in_memory().unwrap();
    let plan: String = store
        .conn()
        .query_row(
            "EXPLAIN QUERY PLAN SELECT id FROM pages WHERE url = 'https://a/'",
            [],
            |r| r.get(3),
        )
        .unwrap();
    assert!(
        plan.contains("USING INDEX") || plan.contains("USING COVERING INDEX"),
        "{plan}"
    );
}

// ---- link graph ----------------------------------------------------------

#[test]
fn links_are_strict_and_belong_to_a_source_page() {
    let store = Store::in_memory().unwrap();
    assert!(table_exists(store.conn(), "links"));

    let err = store.conn().execute(
        "INSERT INTO links (source_page_id, href, target_url, anchor_text, nofollow) \
         VALUES (999, '/missing', 'https://example.com/missing', '', 0)",
        [],
    );
    assert!(
        err.is_err(),
        "a link cannot outlive a nonexistent source page"
    );
}

#[test]
fn both_directions_of_the_link_graph_use_an_index() {
    let store = Store::in_memory().unwrap();

    let outlink_plan: String = store
        .conn()
        .query_row(
            "EXPLAIN QUERY PLAN SELECT target_url FROM links WHERE source_page_id = 1",
            [],
            |r| r.get(3),
        )
        .unwrap();
    assert!(
        outlink_plan.contains("links_source"),
        "outlink lookup did not use its index: {outlink_plan}"
    );

    // The inlink direction is indexed only once the crawl has finished. During
    // a crawl `links_target` would take one random TEXT insert per discovered
    // link — 14M on a 500k crawl — which is what made throughput fall from
    // 3,690 URL/s at 10k to 359 at 500k.
    let inlink_plan = |s: &Store| -> String {
        let mut stmt = s
            .conn()
            .prepare(
                "EXPLAIN QUERY PLAN \
                 SELECT l.source_page_id FROM pages p \
                 JOIN links l ON l.target_url = p.url WHERE p.id = 1",
            )
            .unwrap();
        stmt.query_map([], |r| r.get::<_, String>(3))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join("; ")
    };
    assert!(
        !inlink_plan(&store).contains("links_target"),
        "the inlink index must not be maintained during a crawl"
    );

    store.build_query_indices().unwrap();
    let after = inlink_plan(&store);
    assert!(
        after.contains("links_target"),
        "inlink join did not use its index after the crawl finished: {after}"
    );
}

#[test]
fn the_link_index_can_be_built_before_the_rest() {
    // Site rules run between the crawl loop and `build_query_indices`, and
    // three of them join on `links.target_url`. Without the index at that
    // point SQLite re-scans `links` once per page: measured at 10k pages,
    // `links.orphan-page` alone took 45 s, and the shape is O(pages x links),
    // so 500k would not finish.
    //
    // The issue indices stay behind, because nothing reads those until the
    // file is opened for browsing — the rule the 500k measurement taught.
    let store = Store::in_memory().unwrap();
    let indices = |s: &Store| -> Vec<String> {
        s.conn()
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='index' \
                 AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    store.build_link_index().unwrap();
    let after_link = indices(&store);
    assert!(after_link.iter().any(|n| n == "links_target"));
    assert!(
        !after_link.iter().any(|n| n.starts_with("issues_")),
        "the issue indices are not needed yet: {after_link:?}"
    );

    // And the umbrella call is still idempotent over the index just built.
    store.build_query_indices().unwrap();
    let after_all = indices(&store);
    assert!(after_all.iter().any(|n| n == "issues_rule"));
    assert_eq!(after_all.iter().filter(|n| *n == "links_target").count(), 1);
}

// ---- crawl state ---------------------------------------------------------

#[test]
fn crawl_and_frontier_tables_are_strict() {
    let store = Store::in_memory().unwrap();
    assert!(table_exists(store.conn(), "crawl"));
    assert!(table_exists(store.conn(), "frontier"));

    let err = store.conn().execute(
        "INSERT INTO frontier (url, depth) VALUES ('https://a/', 'deep')",
        [],
    );
    assert!(err.is_err(), "depth is an integer, not arbitrary text");
}

#[test]
fn loading_the_frontier_for_resume_uses_indices() {
    let store = Store::in_memory().unwrap();
    let mut stmt = store
        .conn()
        .prepare(
            "EXPLAIN QUERY PLAN \
             SELECT f.url, f.depth, p.id IS NOT NULL \
             FROM frontier f LEFT JOIN pages p ON p.url = f.url \
             ORDER BY f.depth, f.url",
        )
        .unwrap();
    let plan = stmt
        .query_map([], |r| r.get::<_, String>(3))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
        .join("; ");
    assert!(
        plan.contains("frontier_order") && plan.contains("url"),
        "resume join did not use both URL indices: {plan}"
    );
}

// ---- deferred query indices (the 500k scaling fix) ----------------------

#[test]
fn links_target_is_absent_during_a_crawl() {
    // It is written and never read while crawling, and maintaining a TEXT index
    // over randomly-ordered URLs is what made throughput fall away with scale.
    let store = Store::in_memory().unwrap();
    assert_eq!(
        scalar::<i64>(
            store.conn(),
            "SELECT count(*) FROM sqlite_master WHERE type='index' AND name='links_target'"
        ),
        0
    );
    // links_source stays: it is the outlinks direction and is cheap, being an
    // INTEGER key that arrives in ascending order.
    assert_eq!(
        scalar::<i64>(
            store.conn(),
            "SELECT count(*) FROM sqlite_master WHERE type='index' AND name='links_source'"
        ),
        1
    );
}

#[test]
fn building_query_indices_creates_it_and_is_idempotent() {
    let store = Store::in_memory().unwrap();
    store.build_query_indices().unwrap();
    assert_eq!(
        scalar::<i64>(
            store.conn(),
            "SELECT count(*) FROM sqlite_master WHERE type='index' AND name='links_target'"
        ),
        1
    );
    // Called again on a resumed or re-opened file it must not error.
    store.build_query_indices().unwrap();
}

#[test]
fn the_inlinks_query_uses_the_index_once_it_is_built() {
    // The index has to actually earn its place, or deferring it just loses a
    // capability. This is the query the detail pane will issue.
    let store = Store::in_memory().unwrap();
    let plan = |s: &Store| -> String {
        s.conn()
            .query_row(
                "EXPLAIN QUERY PLAN SELECT id FROM links WHERE target_url = 'https://a/'",
                [],
                |r| r.get(3),
            )
            .unwrap()
    };
    assert!(plan(&store).contains("SCAN"), "{}", plan(&store));

    store.build_query_indices().unwrap();
    let after = plan(&store);
    assert!(
        after.contains("USING INDEX") || after.contains("USING COVERING INDEX"),
        "{after}"
    );
}

#[test]
fn an_older_file_has_the_crawl_time_index_removed_on_open() {
    // Migration 007 runs against a v6 file that still carries links_target.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.pounce");
    {
        let store = Store::open(&path).unwrap();
        store
            .conn()
            .execute_batch("CREATE INDEX IF NOT EXISTS links_target ON links (target_url)")
            .unwrap();
        rewind_to(store.conn(), 6);
    }

    let reopened = Store::open(&path).unwrap();
    assert_eq!(reopened.version().unwrap(), SCHEMA_VERSION);
    assert_eq!(
        scalar::<i64>(
            reopened.conn(),
            "SELECT count(*) FROM sqlite_master WHERE type='index' AND name='links_target'"
        ),
        0,
        "migration 007 should have dropped it"
    );
}

#[test]
fn large_sorts_spill_to_disk_rather_than_memory() {
    // temp_store MEMORY plus the deferred index build took peak RSS from
    // 261 MB to 1,592 MB at 500k, failing the 400 MB gate to save ~10s of a
    // 142s crawl. 0 = default (file), 1 = FILE, 2 = MEMORY.
    let store = Store::in_memory().unwrap();
    let temp_store: i64 = scalar(store.conn(), "PRAGMA temp_store");
    assert_ne!(temp_store, 2, "sorts must be allowed to spill to disk");
}

#[test]
fn the_page_cache_is_left_at_its_default() {
    // Raising it was tried and measured worse on both axes at 500k: 152.8s /
    // 356 MB at 64 MB of cache against 142.6s / 257 MB at the default. Flat
    // memory is the product's headline advantage, so a bigger cache needs a
    // measurement showing it helps before it goes back in.
    let store = Store::in_memory().unwrap();
    let cache: i64 = scalar(store.conn(), "PRAGMA cache_size");
    assert_eq!(cache, -2_000, "page cache should be SQLite's default");
}

#[test]
fn issue_indices_are_deferred_like_every_other_query_index() {
    // An index nothing reads during a crawl is not maintained during one.
    let store = Store::in_memory().unwrap();
    let count = |s: &Store, name: &str| -> i64 {
        scalar(
            s.conn(),
            &format!("SELECT count(*) FROM sqlite_master WHERE type='index' AND name='{name}'"),
        )
    };
    for name in [
        "issues_page",
        "issues_url",
        "issues_rule",
        "issues_severity",
    ] {
        assert_eq!(
            count(&store, name),
            0,
            "{name} must not exist during a crawl"
        );
    }
    store.build_query_indices().unwrap();
    for name in [
        "issues_page",
        "issues_url",
        "issues_rule",
        "issues_severity",
    ] {
        assert_eq!(count(&store, name), 1, "{name} must exist after the crawl");
    }
}

#[test]
fn deleting_a_page_takes_its_issues_with_it() {
    // Without the cascade, a re-crawl that removed a page would leave issues
    // pointing at nothing, and per-rule counts would drift upward forever.
    let store = Store::in_memory().unwrap();
    store
        .conn()
        .execute_batch(
            "INSERT INTO pages (url, status, depth, size, truncated, kind, content_type_mismatch, \
             elapsed_ms, time_to_headers_ms, redirect_chain, h1, h2, noindex, nofollow, noarchive, \
             nosnippet, hreflang, open_graph, images, word_count, title_count) \
             VALUES ('https://a/', 200, 0, 1, 0, 'html', 0, 1, 1, '[]', '[]', '[]', 0, 0, 0, 0, '[]', '[]', '[]', 0, 0); \
             INSERT INTO issues (url, page_id, rule_id, severity, detail) \
             VALUES ('https://a/', (SELECT id FROM pages), 'title.missing', 'critical', NULL);",
        )
        .unwrap();
    store.conn().execute("DELETE FROM pages", []).unwrap();
    assert_eq!(
        scalar::<i64>(store.conn(), "SELECT count(*) FROM issues"),
        0
    );
}
