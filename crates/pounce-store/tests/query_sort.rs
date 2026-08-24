//! T3.2 — a sort is only offered when the pair it forms with the filter is
//! known to be backed.
//!
//! The probe's finding, which this task exists for: an indexed sort column is
//! still 18 seconds when the active filter is a *different* indexed column
//! matching most rows. So the unit of support is the pair, not the column, and
//! the test that matters is the query plan — `links.orphan-page` cost 45
//! seconds because nobody asserted one.

mod common;

use common::seeded;
use pounce_parse::BodyKind;
use pounce_store::{
    Comparison, Filter, FilterKind, FilterSpec, MAX_COMPOSITE_INDICES, QueryError, SortColumn,
    SortDirection, SortSpec, Store,
};

/// Enough rows that the planner prefers an index to a scan; small enough to
/// stay an in-memory test.
const PAGES: u64 = 5_000;

/// A representative value for each filter kind — the *unselective* one, which
/// is the case the composites exist for.
fn filter_of(kind: FilterKind) -> Filter {
    match kind {
        FilterKind::Status => Filter::Status(Comparison::Eq, 200),
        FilterKind::Depth => Filter::Depth(Comparison::Le, 2),
        FilterKind::WordCount => Filter::WordCount(Comparison::Lt, 300),
        FilterKind::Kind => Filter::Kind(BodyKind::Html),
        FilterKind::Noindex => Filter::Noindex(false),
        FilterKind::HasIssue => Filter::HasIssue(None),
        FilterKind::UrlContains => Filter::UrlContains("/blog/".into()),
    }
}

fn plan(store: &Store, filter: Filter, sort: &SortSpec) -> Vec<String> {
    let spec = FilterSpec::new().with(filter);
    let (where_sql, params) = spec.compile();
    let sql = format!(
        "EXPLAIN QUERY PLAN SELECT p.id, p.url, p.status, p.depth, p.size, p.word_count, \
         p.title, p.kind, p.noindex FROM pages p {where_sql} {} LIMIT 200 OFFSET 2000",
        sort.compile()
    );
    store
        .conn()
        .prepare(&sql)
        .unwrap()
        .query_map(rusqlite::params_from_iter(params.iter()), |r| r.get(3))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn every_declared_composite_pair_has_its_index() {
    let store = seeded(PAGES);
    for (filter, sort) in pounce_store::query::composite_pairs() {
        let name = pounce_store::query::composite_index_name(*filter, *sort).unwrap();
        let found: i64 = store
            .conn()
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
                [&name],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(found, 1, "{name} was declared but never built");
    }
}

#[test]
fn no_declared_pair_sorts_with_a_temp_b_tree() {
    // The whole point of the task. `USE TEMP B-TREE FOR ORDER BY` is SQLite
    // saying it will sort the entire match set before it can return the first
    // row of a window — 18 s at 1M in the probe.
    let store = seeded(PAGES);
    for &sort_column in SortColumn::all() {
        for kind in [
            FilterKind::Status,
            FilterKind::Depth,
            FilterKind::WordCount,
            FilterKind::Kind,
            FilterKind::Noindex,
            FilterKind::HasIssue,
            FilterKind::UrlContains,
        ] {
            let spec = FilterSpec::new().with(filter_of(kind));
            let Ok(sort) = SortSpec::new(&spec, sort_column, SortDirection::Asc) else {
                continue; // refused pairs are not promised anything
            };
            let plans = plan(&store, filter_of(kind), &sort);
            assert!(
                !plans.iter().any(|p| p.contains("TEMP B-TREE")),
                "{} + sort {} is offered but sorts with a temp B-tree: {plans:?}",
                kind.name(),
                sort_column.column()
            );
        }
    }
}

#[test]
fn an_unsupported_pair_is_refused_rather_than_run() {
    // A substring filter with a sort on any other column: no index can serve
    // both, so the honest answer is Err, not a slow query.
    let spec = FilterSpec::new().with(Filter::UrlContains("blog".into()));
    assert_eq!(
        SortSpec::new(&spec, SortColumn::WordCount, SortDirection::Asc),
        Err(QueryError::UnsupportedPair {
            filter: "url_contains",
            sort: "word_count"
        })
    );
    // The same filter with a URL sort is supported: the URL index provides the
    // value the pattern is tested against.
    assert!(SortSpec::new(&spec, SortColumn::Url, SortDirection::Asc).is_ok());
}

#[test]
fn one_unsupported_filter_refuses_the_whole_sort() {
    // A spec is ANDed, and one unselective unsupported filter is enough to
    // make the query slow whatever the others do.
    let spec = FilterSpec::new()
        .with(Filter::Status(Comparison::Eq, 200))
        .with(Filter::UrlContains("blog".into()));
    assert!(SortSpec::new(&spec, SortColumn::Size, SortDirection::Asc).is_err());
}

#[test]
fn an_unfiltered_grid_can_sort_by_anything() {
    let spec = FilterSpec::new();
    for &column in SortColumn::all() {
        assert!(
            SortSpec::new(&spec, column, SortDirection::Asc).is_ok(),
            "{} has a single-column index and no filter to fight with",
            column.column()
        );
    }
}

#[test]
fn a_filter_can_always_sort_by_its_own_column() {
    for (kind, column) in [
        (FilterKind::Status, SortColumn::Status),
        (FilterKind::Depth, SortColumn::Depth),
        (FilterKind::WordCount, SortColumn::WordCount),
    ] {
        let spec = FilterSpec::new().with(filter_of(kind));
        assert!(SortSpec::new(&spec, column, SortDirection::Asc).is_ok());
    }
}

#[test]
fn the_sort_carries_an_id_tie_break_in_the_same_direction() {
    // Without a total order, `OFFSET` is not stable: a row can show up in two
    // consecutive windows, or in neither, while the user scrolls.
    let spec = FilterSpec::new();
    let asc = SortSpec::new(&spec, SortColumn::WordCount, SortDirection::Asc).unwrap();
    assert_eq!(asc.compile(), "ORDER BY p.word_count ASC, p.id ASC");
    let desc = SortSpec::new(&spec, SortColumn::WordCount, SortDirection::Desc).unwrap();
    assert_eq!(desc.compile(), "ORDER BY p.word_count DESC, p.id DESC");
}

#[test]
fn the_composite_set_stays_under_its_ceiling() {
    // Each composite is cheap alone; twenty are hundreds of megabytes at 1M
    // rows, on a file the user keeps. The ceiling makes the next one a
    // decision rather than a commit.
    let declared = pounce_store::query::composite_pairs().len();
    assert!(
        declared <= MAX_COMPOSITE_INDICES,
        "{declared} composite indices declared, ceiling is {MAX_COMPOSITE_INDICES}"
    );

    let built: i64 = seeded(PAGES)
        .conn()
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'index' AND name LIKE 'pages_%_%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        built as usize <= MAX_COMPOSITE_INDICES + SortColumn::all().len(),
        "{built} indices on pages — more than the declared set plus the \
         single-column ones the schema ships"
    );
}
