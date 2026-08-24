//! T3.0 — which narrow-row shape, decided by measurement.
//!
//! Two candidate schemas for the grid row, from
//! `docs/plans/2026-08-23-m3-query-layer.md` §2:
//!
//! - **Wide** — today's `pages`, plus the probe's duplicated `row_view` table
//!   built after the crawl. No write-path change; every grid column stored
//!   twice; ~17 s of post-crawl build at 500k.
//! - **Split** — `pages` becomes the narrow grid row and its repeating JSON
//!   columns move to `page_detail`. No duplication, no build step, but **one
//!   extra insert per page on the crawl's hot path**, which is where this
//!   repo's surprises live (deferring one index was worth 9.76x).
//!
//! Both arms are hand-rolled here rather than run through `Writer`, because
//! only one of them can be production at a time and the comparison has to be
//! symmetric. The wide arm's statements mirror `writer::push` exactly — same
//! upsert, same `SELECT id`, same link delete-and-reinsert, same batch size —
//! so the only difference between the arms is the schema and the extra insert.
//!
//! ponytail: this file is an experiment, not a fixture. When T3.0's decision is
//! written into the plan, the losing arm stays only as the numbers in
//! `docs/benchmarks/`.
//!
//! ```text
//! cargo test --release -p pounce-store --test narrow_row_shape -- --ignored --nocapture
//! ```

use pounce_core::CrawlUrl;
use pounce_parse::{BodyKind, Link, MetaRobots, PageRecord};
use rusqlite::{Connection, params};
use std::path::Path;
use std::time::{Duration, Instant};

const BASE: &str = "http://e.com";
const LINKS_PER_PAGE: u64 = 28;
const BATCH: usize = 500;
/// The window the virtualised grid asks for, at the depth the probe used.
const WINDOW: usize = 200;
/// The grid query pages into the middle of the result, as the probe did.
fn deep_offset(pages: u64) -> u64 {
    pages / 2
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Shape {
    /// Today's schema, plus a post-crawl `row_view` copy.
    Wide,
    /// Narrow `pages` + `page_detail`.
    Split,
}

impl Shape {
    /// The table the grid query reads.
    fn grid_table(self) -> &'static str {
        match self {
            Shape::Wide => "row_view",
            Shape::Split => "pages",
        }
    }
}

// --- schema -----------------------------------------------------------------

/// Columns `pages` carries in each shape. The wide list is migration 001 plus
/// 009; the narrow one is it minus the repeating JSON.
const WIDE: &[&str] = &[
    "url",
    "status",
    "depth",
    "size",
    "truncated",
    "content_type",
    "charset",
    "kind",
    "content_type_mismatch",
    "elapsed_ms",
    "time_to_headers_ms",
    "redirect_chain",
    "title",
    "meta_description",
    "h1",
    "h2",
    "canonical",
    "canonical_url",
    "noindex",
    "nofollow",
    "noarchive",
    "nosnippet",
    "hreflang",
    "open_graph",
    "images",
    "word_count",
    "title_count",
    "body_hash",
];

/// The repeating fields that move out of `pages` under `Split`.
const DETAIL: &[&str] = &[
    "redirect_chain",
    "h1",
    "h2",
    "hreflang",
    "open_graph",
    "images",
];

fn narrow() -> Vec<&'static str> {
    WIDE.iter()
        .copied()
        .filter(|c| !DETAIL.contains(c))
        .collect()
}

fn column_type(col: &str) -> &'static str {
    match col {
        "url" => "TEXT NOT NULL UNIQUE",
        "content_type" | "charset" | "title" | "meta_description" | "canonical"
        | "canonical_url" => "TEXT",
        "kind" | "redirect_chain" | "h1" | "h2" | "hreflang" | "open_graph" | "images" => {
            "TEXT NOT NULL"
        }
        "body_hash" => "INTEGER",
        _ => "INTEGER NOT NULL",
    }
}

fn open(path: &Path) -> Connection {
    let conn = Connection::open(path).unwrap();
    // The production pragmas, from `Store::open`/`prepare`. A different
    // journal mode or sync level would measure a different database.
    conn.pragma_update(None, "journal_mode", "WAL").unwrap();
    conn.pragma_update(None, "synchronous", "NORMAL").unwrap();
    conn.pragma_update(None, "foreign_keys", true).unwrap();
    conn
}

fn create_schema(conn: &Connection, shape: Shape) {
    let cols = match shape {
        Shape::Wide => WIDE.to_vec(),
        Shape::Split => narrow(),
    };
    let defs = cols
        .iter()
        .map(|c| format!("{c} {}", column_type(c)))
        .collect::<Vec<_>>()
        .join(",\n  ");
    conn.execute_batch(&format!(
        "CREATE TABLE pages (
  id INTEGER PRIMARY KEY,
  {defs}
) STRICT;
CREATE INDEX pages_status     ON pages (status);
CREATE INDEX pages_depth      ON pages (depth);
CREATE INDEX pages_size       ON pages (size);
CREATE INDEX pages_word_count ON pages (word_count);
CREATE INDEX pages_elapsed    ON pages (elapsed_ms);
CREATE INDEX pages_kind       ON pages (kind);
CREATE INDEX pages_title      ON pages (title);
CREATE INDEX pages_noindex    ON pages (noindex);
-- links_target stays deferred, as migration 007 requires.
CREATE TABLE links (
  id INTEGER PRIMARY KEY,
  source_page_id INTEGER NOT NULL REFERENCES pages (id) ON DELETE CASCADE,
  href TEXT NOT NULL,
  target_url TEXT,
  anchor_text TEXT NOT NULL,
  nofollow INTEGER NOT NULL
) STRICT;
CREATE INDEX links_source ON links (source_page_id);"
    ))
    .unwrap();
    if shape == Shape::Split {
        let defs = DETAIL
            .iter()
            .map(|c| format!("{c} TEXT NOT NULL"))
            .collect::<Vec<_>>()
            .join(",\n  ");
        conn.execute_batch(&format!(
            "CREATE TABLE page_detail (
  page_id INTEGER PRIMARY KEY REFERENCES pages (id) ON DELETE CASCADE,
  {defs}
) STRICT;"
        ))
        .unwrap();
    }
}

/// The upsert, built the way `writer::insert_sql` builds it.
fn upsert_sql(table: &str, key: &str, cols: &[&str]) -> String {
    let list = cols.join(", ");
    let holes = (1..=cols.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let updates = cols
        .iter()
        .filter(|c| **c != key)
        .map(|c| format!("{c} = excluded.{c}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "INSERT INTO {table} ({list}) VALUES ({holes}) \
         ON CONFLICT({key}) DO UPDATE SET {updates}"
    )
}

// --- records ----------------------------------------------------------------

fn path_of(i: u64) -> String {
    format!("/section/word-{i}")
}

/// Fixture-shaped: ~28 links, unique title and description, five images, a
/// word count that varies so the sort has work to do.
fn record(i: u64, pages: u64) -> PageRecord {
    let url = format!("{BASE}{}", path_of(i));
    PageRecord {
        url: CrawlUrl::parse(&url).unwrap(),
        status: 200,
        depth: (i % 5) as u16,
        size: 18_000 + (i % 4_000) as usize,
        truncated: false,
        content_type: Some("text/html".into()),
        charset: Some("utf-8".into()),
        kind: BodyKind::Html,
        content_type_mismatch: false,
        elapsed_ms: (7 + i % 40) as u32,
        time_to_headers_ms: 3,
        redirect_chain: vec![],
        title: Some(format!("Title number {i} with a few more words")),
        title_count: 1,
        meta_description: Some(format!(
            "Description {i}: a sentence or so of ordinary summary text."
        )),
        h1: vec![format!("A heading naming page {i}'s subject")],
        h2: (0..3)
            .map(|k| format!("Subheading {k} of page {i}"))
            .collect(),
        canonical: None,
        canonical_url: None,
        meta_robots: MetaRobots::default(),
        hreflang: vec![],
        open_graph: vec![],
        links: (0..LINKS_PER_PAGE)
            .map(|k| {
                let t = path_of(2 + (i + k * 7 + 1) % pages);
                Link {
                    href: t.clone(),
                    target: Some(CrawlUrl::parse(&format!("{BASE}{t}")).unwrap()),
                    text: "somewhere else".into(),
                    nofollow: false,
                }
            })
            .collect(),
        images: (0..5)
            .map(|k| pounce_parse::Image {
                src: format!("/static/img-{k}.jpg"),
                alt: Some(format!("An image {k} on page {i}")),
            })
            .collect(),
        word_count: 100 + (i * 37 % 900) as u32,
        body_hash: Some(i.wrapping_mul(0x9E37_79B9_7F4A_7C15)),
    }
}

fn json<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap()
}

// --- the write arm ----------------------------------------------------------

/// Writes every record through one shape, returning the time it took.
///
/// The structure is `writer::push`: one open transaction per `BATCH` rows, an
/// upsert, the `SELECT id` that follows it, and the link delete-and-reinsert.
fn write_arm(path: &Path, records: &[PageRecord], shape: Shape) -> Duration {
    let conn = open(path);
    create_schema(&conn, shape);

    let cols = match shape {
        Shape::Wide => WIDE.to_vec(),
        Shape::Split => narrow(),
    };
    let page_sql = upsert_sql("pages", "url", &cols);
    let detail_cols: Vec<&str> = std::iter::once("page_id")
        .chain(DETAIL.iter().copied())
        .collect();
    let detail_sql = upsert_sql("page_detail", "page_id", &detail_cols);

    let start = Instant::now();
    conn.execute_batch("BEGIN").unwrap();
    for (n, r) in records.iter().enumerate() {
        let kind = serde_json::to_value(r.kind)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| "undeclared".into());
        let robots = r.meta_robots;

        // Bound in `WIDE` order; the narrow arm skips the six JSON columns.
        let mut stmt = conn.prepare_cached(&page_sql).unwrap();
        let mut values: Vec<Box<dyn rusqlite::ToSql>> = Vec::with_capacity(cols.len());
        for col in &cols {
            values.push(match *col {
                "url" => Box::new(r.url.to_string()),
                "status" => Box::new(r.status),
                "depth" => Box::new(r.depth),
                "size" => Box::new(r.size as i64),
                "truncated" => Box::new(r.truncated),
                "content_type" => Box::new(r.content_type.clone()),
                "charset" => Box::new(r.charset.clone()),
                "kind" => Box::new(kind.clone()),
                "content_type_mismatch" => Box::new(r.content_type_mismatch),
                "elapsed_ms" => Box::new(r.elapsed_ms),
                "time_to_headers_ms" => Box::new(r.time_to_headers_ms),
                "redirect_chain" => Box::new(json(&r.redirect_chain)),
                "title" => Box::new(r.title.clone()),
                "meta_description" => Box::new(r.meta_description.clone()),
                "h1" => Box::new(json(&r.h1)),
                "h2" => Box::new(json(&r.h2)),
                "canonical" => Box::new(r.canonical.clone()),
                "canonical_url" => Box::new(r.canonical_url.as_ref().map(ToString::to_string)),
                "noindex" => Box::new(robots.noindex),
                "nofollow" => Box::new(robots.nofollow),
                "noarchive" => Box::new(robots.noarchive),
                "nosnippet" => Box::new(robots.nosnippet),
                "hreflang" => Box::new(json(&r.hreflang)),
                "open_graph" => Box::new(json(&r.open_graph)),
                "images" => Box::new(json(&r.images)),
                "word_count" => Box::new(r.word_count),
                "title_count" => Box::new(r.title_count),
                "body_hash" => Box::new(r.body_hash.map(|h| h as i64)),
                other => unreachable!("unbound column {other}"),
            });
        }
        stmt.execute(rusqlite::params_from_iter(
            values.iter().map(|v| v.as_ref()),
        ))
        .unwrap();
        drop(stmt);

        let page_id: i64 = conn
            .query_row(
                "SELECT id FROM pages WHERE url = ?1",
                [r.url.to_string()],
                |row| row.get(0),
            )
            .unwrap();

        // The whole question T3.0 asks: what does this second insert cost?
        if shape == Shape::Split {
            conn.prepare_cached(&detail_sql)
                .unwrap()
                .execute(params![
                    page_id,
                    json(&r.redirect_chain),
                    json(&r.h1),
                    json(&r.h2),
                    json(&r.hreflang),
                    json(&r.open_graph),
                    json(&r.images),
                ])
                .unwrap();
        }

        conn.execute("DELETE FROM links WHERE source_page_id = ?1", [page_id])
            .unwrap();
        let mut stmt = conn
            .prepare_cached(
                "INSERT INTO links (source_page_id, href, target_url, anchor_text, nofollow) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .unwrap();
        for link in &r.links {
            stmt.execute(params![
                page_id,
                link.href,
                link.target.as_ref().map(ToString::to_string),
                link.text,
                link.nofollow,
            ])
            .unwrap();
        }
        drop(stmt);

        if (n + 1) % BATCH == 0 {
            conn.execute_batch("COMMIT; BEGIN").unwrap();
        }
    }
    conn.execute_batch("COMMIT").unwrap();
    let elapsed = start.elapsed();

    // Both arms must have produced the same crawl, or the timings compare two
    // different amounts of work.
    let pages: u64 = conn
        .query_row("SELECT count(*) FROM pages", [], |r| r.get(0))
        .unwrap();
    let links: u64 = conn
        .query_row("SELECT count(*) FROM links", [], |r| r.get(0))
        .unwrap();
    assert_eq!(pages, records.len() as u64, "{shape:?} page count");
    assert_eq!(
        links,
        records.len() as u64 * LINKS_PER_PAGE,
        "{shape:?} link count"
    );
    if shape == Shape::Split {
        let detail: u64 = conn
            .query_row("SELECT count(*) FROM page_detail", [], |r| r.get(0))
            .unwrap();
        assert_eq!(detail, pages, "a detail row per page");
    }
    elapsed
}

// --- the query arm ----------------------------------------------------------

/// Seeds `pages` rows in one shape, streaming — no `PageRecord` is kept, so a
/// million rows costs a million rows of disk and nothing of memory.
///
/// Links are deliberately absent: the grid query never reads them, and at 1M
/// they are 28M rows of setup that would change no measured number.
fn seed_query_shape(path: &Path, shape: Shape, pages: u64) {
    let conn = open(path);
    create_schema(&conn, shape);
    let cols = match shape {
        Shape::Wide => WIDE.to_vec(),
        Shape::Split => narrow(),
    };
    let sql = upsert_sql("pages", "url", &cols);
    let detail_cols: Vec<&str> = std::iter::once("page_id")
        .chain(DETAIL.iter().copied())
        .collect();
    let detail_sql = upsert_sql("page_detail", "page_id", &detail_cols);

    conn.execute_batch("BEGIN").unwrap();
    for i in 1..=pages {
        let r = record(i, pages);
        // Same binding as the write arm, minus its links.
        let kind = "html";
        let mut values: Vec<Box<dyn rusqlite::ToSql>> = Vec::with_capacity(cols.len());
        for col in &cols {
            values.push(match *col {
                "url" => Box::new(r.url.to_string()),
                "status" => Box::new(r.status),
                "depth" => Box::new(r.depth),
                "size" => Box::new(r.size as i64),
                "truncated" => Box::new(r.truncated),
                "content_type" => Box::new(r.content_type.clone()),
                "charset" => Box::new(r.charset.clone()),
                "kind" => Box::new(kind),
                "content_type_mismatch" => Box::new(r.content_type_mismatch),
                "elapsed_ms" => Box::new(r.elapsed_ms),
                "time_to_headers_ms" => Box::new(r.time_to_headers_ms),
                "redirect_chain" => Box::new(json(&r.redirect_chain)),
                "title" => Box::new(r.title.clone()),
                "meta_description" => Box::new(r.meta_description.clone()),
                "h1" => Box::new(json(&r.h1)),
                "h2" => Box::new(json(&r.h2)),
                "canonical" => Box::new(r.canonical.clone()),
                "canonical_url" => Box::new(None::<String>),
                "noindex" => Box::new(false),
                "nofollow" => Box::new(false),
                "noarchive" => Box::new(false),
                "nosnippet" => Box::new(false),
                "hreflang" => Box::new(json(&r.hreflang)),
                "open_graph" => Box::new(json(&r.open_graph)),
                "images" => Box::new(json(&r.images)),
                "word_count" => Box::new(r.word_count),
                "title_count" => Box::new(r.title_count),
                "body_hash" => Box::new(r.body_hash.map(|h| h as i64)),
                other => unreachable!("unbound column {other}"),
            });
        }
        conn.prepare_cached(&sql)
            .unwrap()
            .execute(rusqlite::params_from_iter(
                values.iter().map(|v| v.as_ref()),
            ))
            .unwrap();
        if shape == Shape::Split {
            conn.prepare_cached(&detail_sql)
                .unwrap()
                .execute(params![
                    i as i64,
                    json(&r.redirect_chain),
                    json(&r.h1),
                    json(&r.h2),
                    json(&r.hreflang),
                    json(&r.open_graph),
                    json(&r.images),
                ])
                .unwrap();
        }
        if i % BATCH as u64 == 0 {
            conn.execute_batch("COMMIT; BEGIN").unwrap();
        }
    }
    conn.execute_batch("COMMIT").unwrap();

    // The post-crawl work each shape needs before the grid can query it. The
    // wide shape has to build its copy; the split shape already is one.
    let build = Instant::now();
    if shape == Shape::Wide {
        // `id INTEGER PRIMARY KEY` rather than `CREATE TABLE ... AS`: the copy
        // has to be a rowid table keyed like `pages`, or the index tail is not
        // the id and the tie-break sort falls back to a temp B-tree.
        conn.execute_batch(
            "CREATE TABLE row_view (
               id INTEGER PRIMARY KEY,
               url TEXT NOT NULL UNIQUE,
               status INTEGER NOT NULL,
               depth INTEGER NOT NULL,
               size INTEGER NOT NULL,
               word_count INTEGER NOT NULL,
               title TEXT,
               kind TEXT NOT NULL,
               noindex INTEGER NOT NULL
             ) STRICT;
             INSERT INTO row_view
               SELECT id, url, status, depth, size, word_count, title, kind, noindex
               FROM pages;",
        )
        .unwrap();
    }
    conn.execute_batch(&format!(
        "CREATE INDEX grid_status_word ON {} (status, word_count)",
        shape.grid_table()
    ))
    .unwrap();
    eprintln!("  {shape:?}: post-crawl build {:?}", build.elapsed());
    conn.pragma_update(None, "wal_checkpoint", "TRUNCATE").ok();
}

/// The probe's worst case: an unselective filter and a sort on another column,
/// paged into the middle of the result.
fn grid_sql(shape: Shape, pages: u64) -> String {
    format!(
        "SELECT id, url, status, depth, size, word_count, title, kind, noindex \
         FROM {} WHERE status = 200 ORDER BY word_count, id LIMIT {WINDOW} OFFSET {}",
        shape.grid_table(),
        deep_offset(pages)
    )
}

fn run_grid(conn: &Connection, shape: Shape, pages: u64) -> (Duration, Vec<i64>) {
    let sql = grid_sql(shape, pages);
    let start = Instant::now();
    let ids: Vec<i64> = conn
        .prepare_cached(&sql)
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    (start.elapsed(), ids)
}

fn plan(conn: &Connection, sql: &str) -> Vec<String> {
    conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
        .unwrap()
        .query_map([], |r| r.get::<_, String>(3))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn file_bytes(path: &Path) -> u64 {
    let mut total = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    for suffix in ["-wal", "-shm"] {
        let side = path.with_file_name(format!(
            "{}{suffix}",
            path.file_name().unwrap().to_string_lossy()
        ));
        total += std::fs::metadata(side).map(|m| m.len()).unwrap_or(0);
    }
    total
}

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

fn env_or(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

// --- the measurements -------------------------------------------------------

#[test]
#[ignore = "minutes of seeding; run with --release --ignored --nocapture"]
fn write_throughput_ab() {
    let pages = env_or("NARROW_ROW_PAGES", 100_000);
    let pairs = env_or("NARROW_ROW_PAIRS", 5);

    eprintln!("generating {pages} records...");
    let records: Vec<PageRecord> = (1..=pages).map(|i| record(i, pages)).collect();

    let (mut wide, mut split) = (Vec::new(), Vec::new());
    for pair in 0..pairs {
        // Interleaved, and the order alternates: a machine that warms up or
        // thermally throttles would otherwise hand the advantage to whichever
        // arm always runs first.
        let order = if pair % 2 == 0 {
            [Shape::Wide, Shape::Split]
        } else {
            [Shape::Split, Shape::Wide]
        };
        for shape in order {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("ab.pounce");
            let took = write_arm(&path, &records, shape);
            eprintln!(
                "pair {pair} {shape:?}: {took:?}  ({:.0} pages/s, {} MB)",
                pages as f64 / took.as_secs_f64(),
                file_bytes(&path) / 1_048_576
            );
            match shape {
                Shape::Wide => wide.push(took),
                Shape::Split => split.push(took),
            }
        }
    }

    let (a, b) = (median(wide), median(split));
    let delta = (b.as_secs_f64() - a.as_secs_f64()) / a.as_secs_f64() * 100.0;
    eprintln!("\n=== write path, {pages} pages, medians of {pairs} ===");
    eprintln!("  wide (today + post-crawl row_view): {a:?}");
    eprintln!("  split (pages + page_detail):        {b:?}");
    eprintln!("  split costs {delta:+.2}% of store write time");
}

#[test]
#[ignore = "seeds two databases of up to 1M rows; run with --release --ignored --nocapture"]
fn grid_query_ab() {
    let pages = env_or("NARROW_ROW_QUERY_PAGES", 1_000_000);
    assert!(
        pages > deep_offset(pages) + WINDOW as u64,
        "the deep-offset window must land inside the seeded rows"
    );
    let dir = tempfile::tempdir().unwrap();

    let mut results = Vec::new();
    for shape in [Shape::Wide, Shape::Split] {
        let path = dir.path().join(format!("{shape:?}.pounce"));
        eprintln!("seeding {pages} rows, {shape:?}...");
        let seeded = Instant::now();
        seed_query_shape(&path, shape, pages);
        eprintln!("  seeded in {:?}", seeded.elapsed());

        let conn = open(&path);
        let plans = plan(&conn, &grid_sql(shape, pages));
        eprintln!("  plan: {plans:?}");
        assert!(
            !plans.iter().any(|p| p.contains("TEMP B-TREE")),
            "{shape:?} sorts with a temp B-tree — the composite index is not serving the query: {plans:?}"
        );

        // Best of three: the first run pays for a cold page cache, and the
        // grid's second scroll does not.
        let mut runs = Vec::new();
        let mut ids = Vec::new();
        for _ in 0..3 {
            let (took, got) = run_grid(&conn, shape, pages);
            ids = got;
            runs.push(took);
        }
        assert_eq!(ids.len(), WINDOW, "{shape:?} returned a short window");
        results.push((shape, median(runs), file_bytes(&path), ids));
    }

    // The two shapes must be answering the same question.
    assert_eq!(
        results[0].3, results[1].3,
        "the shapes disagree about which rows are in the window"
    );

    eprintln!(
        "\n=== grid query, {pages} rows, offset {} ===",
        deep_offset(pages)
    );
    for (shape, took, bytes, _) in &results {
        eprintln!("  {shape:?}: {took:?}   file {} MB", bytes / 1_048_576);
    }
}
