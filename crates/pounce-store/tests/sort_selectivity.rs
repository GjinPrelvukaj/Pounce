//! T3.2's input: which filter × sort pairs need a composite index, and what
//! the declared set costs.
//!
//! The probe established that an indexed sort column is still 18 s when the
//! active filter is a different indexed column matching most rows. It did not
//! establish *which* filters are like that. The full cross product is 49
//! indices and is not an option, so the declared set comes from counting.
//!
//! Three arms, in order:
//!
//! 1. **baseline** — single-column indices only, every pair timed;
//! 2. **sort-first covering** — one index per sort column carrying the filter
//!    columns after it. The cheap-looking alternative to 20 composites, and it
//!    does not work: SQLite takes it for the equality search and then sorts in
//!    a temp B-tree anyway;
//! 3. **declared composites** — what the code ships, with its size.
//!
//! ```text
//! cargo test --release -p pounce-store --test sort_selectivity -- --ignored --nocapture
//! ```

mod common;

use pounce_parse::BodyKind;
use pounce_store::{Comparison, Filter, FilterSpec, SortColumn, Store};
use std::path::Path;
use std::time::Instant;

fn candidates() -> Vec<(&'static str, Filter)> {
    vec![
        ("status=200", Filter::Status(Comparison::Eq, 200)),
        ("status>=400", Filter::Status(Comparison::Ge, 400)),
        ("depth<=2", Filter::Depth(Comparison::Le, 2)),
        ("word_count<300", Filter::WordCount(Comparison::Lt, 300)),
        ("kind=html", Filter::Kind(BodyKind::Html)),
        ("noindex=false", Filter::Noindex(false)),
        ("noindex=true", Filter::Noindex(true)),
        ("has_issue", Filter::HasIssue(None)),
        ("url~/blog/", Filter::UrlContains("/blog/".into())),
    ]
}

/// Times one window, deep enough that a temp B-tree over the match set cannot
/// hide behind an early `LIMIT`. Returns milliseconds and whether the window
/// landed inside the match set at all.
fn time_window(store: &Store, filter: &Filter, sort: SortColumn, offset: u64) -> (f64, bool) {
    let (where_sql, params) = FilterSpec::new().with(filter.clone()).compile();
    let sql = format!(
        "SELECT p.id, p.url, p.status, p.depth, p.size, p.word_count, p.title, p.kind, p.noindex \
         FROM pages p {where_sql} ORDER BY p.{}, p.id LIMIT 200 OFFSET {offset}",
        sort.column()
    );
    let start = Instant::now();
    let rows = store
        .conn()
        .prepare_cached(&sql)
        .unwrap()
        .query_map(rusqlite::params_from_iter(params.iter()), |r| {
            r.get::<_, i64>(0)
        })
        .unwrap()
        .count();
    (start.elapsed().as_secs_f64() * 1000.0, rows > 0)
}

fn table(store: &Store, offset: u64, title: &str) {
    eprintln!("\n=== {title} (ms), offset {offset} ===");
    eprint!("{:16}", "");
    for sort in SortColumn::all() {
        eprint!("{:>12}", sort.column());
    }
    eprintln!();
    for (name, filter) in candidates() {
        eprint!("{name:16}");
        for &sort in SortColumn::all() {
            let (ms, filled) = time_window(store, &filter, sort, offset);
            eprint!("{ms:>11.1}{}", if filled { " " } else { "*" });
        }
        eprintln!();
    }
    eprintln!("* the window fell past the end of the match set");
}

/// Drops indices by name.
///
/// By name rather than by `LIKE 'pages_%_%'`: `_` is a wildcard in `LIKE`, and
/// the escaped form needs an `ESCAPE` clause SQLite will not assume. The first
/// version of this dropped nothing, and the "baseline" arm was quietly the
/// composite arm measured twice.
fn drop_indices(store: &Store, names: impl IntoIterator<Item = String>) {
    for name in names {
        store
            .conn()
            .execute_batch(&format!("DROP INDEX IF EXISTS {name}"))
            .unwrap();
    }
}

fn declared_index_names() -> Vec<String> {
    pounce_store::query::composite_pairs()
        .iter()
        .filter_map(|(f, s)| pounce_store::query::composite_index_name(*f, *s))
        .collect()
}

/// Size after a `VACUUM`, so a dropped index actually leaves the file.
fn compacted_bytes(store: &Store, dir: &Path) -> u64 {
    store.conn().execute_batch("VACUUM").unwrap();
    db_bytes(dir)
}

/// Every byte the database occupies, WAL included.
fn db_bytes(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

#[test]
#[ignore = "seeds a few hundred thousand pages; run with --release --ignored --nocapture"]
fn how_selective_is_each_filter_and_what_does_each_pair_cost() {
    let pages: u64 = std::env::var("SELECTIVITY_PAGES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200_000);
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path().join("selectivity.pounce")).unwrap();

    let seeded = Instant::now();
    common::seed(&mut store, pages);
    eprintln!("seeded {pages} pages in {:?}", seeded.elapsed());
    let with_composites = compacted_bytes(&store, dir.path());

    eprintln!("\n=== selectivity ===");
    for (name, filter) in candidates() {
        let matched = common::count_matching(&store, &FilterSpec::new().with(filter));
        eprintln!(
            "  {name:16} {matched:>9} rows  {:>6.1}%",
            matched as f64 / pages as f64 * 100.0
        );
    }

    let offset = pages / 2;

    // 1. Baseline: the schema's single-column indices only.
    drop_indices(&store, declared_index_names());
    let baseline_bytes = compacted_bytes(&store, dir.path());
    table(&store, offset, "single-column indices only");
    eprintln!(
        "  file {} MB without the composites, {} MB with them — they cost {} MB at {pages} pages",
        baseline_bytes / 1_048_576,
        with_composites / 1_048_576,
        with_composites.saturating_sub(baseline_bytes) / 1_048_576
    );

    // 2. The alternative that looks cheaper and is not.
    for sort in SortColumn::all() {
        store
            .conn()
            .execute_batch(&format!(
                "CREATE INDEX sortfirst_{0} ON pages ({0}, status, kind, noindex, depth, word_count)",
                sort.column()
            ))
            .unwrap();
    }
    table(
        &store,
        offset,
        "sort-first covering indices (one per sort column)",
    );
    eprintln!(
        "  plan for status=200 ORDER BY url: {:?}",
        plan_of(&store, "p.url")
    );
    drop_indices(
        &store,
        SortColumn::all()
            .iter()
            .map(|s| format!("sortfirst_{}", s.column())),
    );

    // 3. What ships.
    store.build_query_indices().unwrap();
    table(&store, offset, "declared composites");
    for sort in SortColumn::all() {
        eprintln!(
            "  plan for status=200 ORDER BY {}: {:?}",
            sort.column(),
            plan_of(&store, &format!("p.{}", sort.column()))
        );
    }
}

fn plan_of(store: &Store, sort_sql: &str) -> Vec<String> {
    store
        .conn()
        .prepare(&format!(
            "EXPLAIN QUERY PLAN SELECT p.id, p.url FROM pages p WHERE p.status = 200 \
             ORDER BY {sort_sql}, p.id LIMIT 200 OFFSET 100000"
        ))
        .unwrap()
        .query_map([], |r| r.get(3))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}
