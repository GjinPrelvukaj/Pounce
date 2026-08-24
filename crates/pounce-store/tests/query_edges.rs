//! The query layer's edges: the inputs a UI will eventually send by accident,
//! and the files a user will eventually open.
//!
//! A `.pounce` file is a document someone keeps, mails and edits. Nothing here
//! may panic on one — an error the caller can show is fine, a crash is not.

mod common;

use common::seeded;
use pounce_store::{
    Comparison, Filter, FilterSpec, SortColumn, SortDirection, SortSpec, Store, Writer,
};

const PAGES: u64 = 500;

fn unfiltered(column: SortColumn) -> (FilterSpec, SortSpec) {
    let filters = FilterSpec::new();
    let sort = SortSpec::new(&filters, column, SortDirection::Asc).unwrap();
    (filters, sort)
}

// ---- offsets and limits ---------------------------------------------------

#[test]
fn an_absurd_offset_returns_nothing_rather_than_wrapping() {
    // `OFFSET` is bound as a signed integer, and a UI that computes a scroll
    // position from a stale total can hand this anything. Wrapped to negative,
    // SQLite silently treats it as zero — the grid would jump to the top
    // instead of showing an empty tail.
    let store = seeded(PAGES);
    let (filters, sort) = unfiltered(SortColumn::Url);
    for offset in [u64::MAX, u64::MAX / 2, i64::MAX as u64 + 1] {
        let page = store.query_rows(&filters, &sort, offset, 10).unwrap();
        assert!(
            page.rows.is_empty(),
            "offset {offset} wrapped and returned rows from the start"
        );
        assert_eq!(page.total, PAGES);
    }
}

#[test]
fn a_zero_limit_is_a_legal_empty_window() {
    // The grid asks for this while it is measuring itself, before it knows how
    // many rows fit.
    let store = seeded(PAGES);
    let (filters, sort) = unfiltered(SortColumn::Url);
    let page = store.query_rows(&filters, &sort, 0, 0).unwrap();
    assert!(page.rows.is_empty());
    assert_eq!(page.total, PAGES, "the total is still the answer");
}

// ---- the shape of the data ------------------------------------------------

#[test]
fn null_titles_sort_together_and_do_not_vanish() {
    // A missing title is a finding, so the rows carrying one must be reachable
    // by scrolling rather than filtered out by the sort.
    let mut store = Store::in_memory().unwrap();
    common::seed(&mut store, 20);
    store
        .conn()
        .execute("UPDATE pages SET title = NULL WHERE id % 3 = 0", [])
        .unwrap();

    let (filters, sort) = unfiltered(SortColumn::Title);
    let page = store.query_rows(&filters, &sort, 0, 100).unwrap();
    assert_eq!(page.rows.len(), 20, "every row is still reachable");
    assert_eq!(page.total, 20);
    // SQLite sorts NULL first ascending; what matters is that they are together
    // and present, not which end they land on.
    let nulls: Vec<usize> = page
        .rows
        .iter()
        .enumerate()
        .filter(|(_, r)| r.title.is_none())
        .map(|(i, _)| i)
        .collect();
    assert!(!nulls.is_empty());
    assert_eq!(
        nulls.last().unwrap() - nulls.first().unwrap(),
        nulls.len() - 1,
        "the untitled rows are not contiguous: {nulls:?}"
    );
}

#[test]
fn an_empty_title_is_not_a_missing_one_through_the_query() {
    // The invariant, held all the way out to the grid row.
    let mut store = Store::in_memory().unwrap();
    common::seed(&mut store, 5);
    store
        .conn()
        .execute("UPDATE pages SET title = '' WHERE id = 1", [])
        .unwrap();
    store
        .conn()
        .execute("UPDATE pages SET title = NULL WHERE id = 2", [])
        .unwrap();

    let (filters, sort) = unfiltered(SortColumn::Url);
    let rows = store.query_rows(&filters, &sort, 0, 10).unwrap().rows;
    let by_id = |id: i64| rows.iter().find(|r| r.id == id).unwrap();
    assert_eq!(by_id(1).title.as_deref(), Some(""));
    assert_eq!(by_id(2).title, None);
}

#[test]
fn a_url_with_wildcards_or_unicode_is_matched_literally_enough() {
    let mut store = Store::in_memory().unwrap();
    common::seed(&mut store, 5);
    {
        let mut writer = Writer::with_batch_size(&mut store, 10);
        for url in [
            "http://e.com/a_b/page",
            "http://e.com/100%25-off/deal",
            "http://e.com/café/münchen",
            "http://e.com/emoji/🦅",
        ] {
            let mut record = common::record_for(url);
            record.title = Some("Edge".into());
            writer.push(&record).unwrap();
        }
        writer.flush().unwrap();
    }

    let count = |needle: &str| {
        let filters = FilterSpec::new().with(Filter::UrlContains(needle.into()));
        let sort = SortSpec::new(&filters, SortColumn::Url, SortDirection::Asc).unwrap();
        store.query_rows(&filters, &sort, 0, 100).unwrap().total
    };

    assert_eq!(count("café"), 1, "a non-ASCII needle matches");
    assert_eq!(count("🦅"), 1, "an astral-plane needle matches");
    assert_eq!(count("100%25"), 1, "a percent-encoded URL is findable");
    // `_` is a LIKE wildcard and is documented as widening the search rather
    // than being escaped — a search box that quietly matched nothing for a
    // URL containing an underscore would be worse.
    assert!(
        count("a_b") >= 1,
        "an underscore needle still finds its URL"
    );
}

// ---- files that have been through something -------------------------------

#[test]
fn a_status_out_of_range_is_an_error_not_a_panic() {
    // STRICT stops a *text* status, not an out-of-range integer, and a
    // `.pounce` file is a document someone can edit.
    let mut store = Store::in_memory().unwrap();
    common::seed(&mut store, 5);
    store
        .conn()
        .execute("UPDATE pages SET status = 70000 WHERE id = 3", [])
        .unwrap();

    let (filters, sort) = unfiltered(SortColumn::Url);
    let result = store.query_rows(&filters, &sort, 0, 10);
    assert!(
        result.is_err(),
        "a status past u16 must be reported, not silently truncated"
    );
}

#[test]
fn a_crawl_that_never_finished_can_still_be_queried() {
    // No `build_query_indices`: the crawl was interrupted, or the file is being
    // read mid-crawl. Slower, and it must still be correct.
    let mut store = Store::in_memory().unwrap();
    common::seed_pages_only(&mut store, 100);

    let (filters, sort) = unfiltered(SortColumn::WordCount);
    let page = store.query_rows(&filters, &sort, 10, 10).unwrap();
    assert_eq!(page.total, 100);
    assert_eq!(page.rows.len(), 10);

    let overview = store.issue_overview().unwrap();
    assert!(overview.total_issues > 0);
}

#[test]
fn an_empty_database_answers_every_query() {
    let store = Store::in_memory().unwrap();
    for &column in SortColumn::all() {
        let (filters, sort) = unfiltered(column);
        let page = store.query_rows(&filters, &sort, 0, 50).unwrap();
        assert_eq!(page.total, 0);
        assert!(page.rows.is_empty());
    }
    assert_eq!(store.issue_overview().unwrap().total_issues, 0);
}

#[test]
fn building_the_query_indices_twice_is_a_no_op() {
    // Called after the crawl, and again by anything that reopens the file.
    let mut store = Store::in_memory().unwrap();
    common::seed(&mut store, 20);
    let before = index_count(&store);
    store.build_query_indices().unwrap();
    store.build_query_indices().unwrap();
    assert_eq!(index_count(&store), before);
}

fn index_count(store: &Store) -> i64 {
    store
        .conn()
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'index'",
            [],
            |r| r.get(0),
        )
        .unwrap()
}

// ---- filters that contradict each other -----------------------------------

#[test]
fn contradictory_filters_return_nothing_rather_than_erroring() {
    let store = seeded(PAGES);
    let filters = FilterSpec::new()
        .with(Filter::Status(Comparison::Eq, 200))
        .with(Filter::Status(Comparison::Eq, 404));
    let sort = SortSpec::new(&filters, SortColumn::Status, SortDirection::Asc).unwrap();
    let page = store.query_rows(&filters, &sort, 0, 10).unwrap();
    assert_eq!(page.total, 0);
    assert!(page.rows.is_empty());
}

#[test]
fn many_filters_at_once_still_compile_and_run() {
    // The UI can stack every filter it offers; nothing here has a limit, and
    // the parameter numbering has to keep up with the limit and offset bound
    // after it.
    let store = seeded(PAGES);
    let filters = FilterSpec::new()
        .with(Filter::Status(Comparison::Eq, 200))
        .with(Filter::Noindex(false))
        .with(Filter::Kind(pounce_parse::BodyKind::Html))
        .with(Filter::WordCount(Comparison::Gt, 0))
        .with(Filter::Depth(Comparison::Le, 4))
        .with(Filter::HasIssue(None));
    let sort = SortSpec::new(&filters, SortColumn::Status, SortDirection::Asc).unwrap();
    let page = store.query_rows(&filters, &sort, 0, 10).unwrap();

    let expected = (1..=PAGES)
        .filter(|i| {
            common::status_of(*i) == 200
                && !common::noindex_of(*i)
                && common::kind_of(*i) == pounce_parse::BodyKind::Html
                && common::issue_of(*i).is_some()
        })
        .count() as u64;
    assert_eq!(page.total, expected);
    assert!(page.total > 0, "the stacked filter must match something");
}
