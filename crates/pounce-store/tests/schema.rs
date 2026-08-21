//! Opening, migrating, and reopening a crawl database.

use pounce_store::schema::SCHEMA_VERSION;
use pounce_store::{Store, StoreError};
use rusqlite::Connection;

fn scalar<T: rusqlite::types::FromSql>(conn: &Connection, sql: &str) -> T {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
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
        store
            .conn()
            .execute_batch("DROP TABLE frontier; DROP TABLE crawl; PRAGMA user_version = 2;")
            .unwrap();
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

    let mut stmt = store
        .conn()
        .prepare(
            "EXPLAIN QUERY PLAN \
             SELECT l.source_page_id FROM pages p \
             JOIN links l ON l.target_url = p.url WHERE p.id = 1",
        )
        .unwrap();
    let inlink_plan = stmt
        .query_map([], |r| r.get::<_, String>(3))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
        .join("; ");
    assert!(
        inlink_plan.contains("links_target"),
        "inlink join did not use its index: {inlink_plan}"
    );
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
