//! T3.3 — the window, and only the window.
//!
//! The spec's load-bearing claim is that the UI never receives the dataset: it
//! asks for the visible rows and gets those. These tests hold the two halves of
//! that — the window is the right slice, and it cannot be made large enough to
//! become the dataset.

mod common;

use common::seeded;
use pounce_store::{
    Comparison, Filter, FilterSpec, MAX_WINDOW, SortColumn, SortDirection, SortSpec, Store, Writer,
};

const PAGES: u64 = 2_000;

fn sorted_by_word_count() -> Vec<u64> {
    // The expectation computed in Rust, with the same total order the SQL uses:
    // word_count, then id.
    let mut ids: Vec<u64> = (1..=PAGES).collect();
    ids.sort_by_key(|i| (common::word_count_of(*i), *i));
    ids
}

fn spec_and_sort(column: SortColumn, direction: SortDirection) -> (FilterSpec, SortSpec) {
    let filters = FilterSpec::new();
    let sort = SortSpec::new(&filters, column, direction).unwrap();
    (filters, sort)
}

#[test]
fn the_window_is_the_slice_it_claims_to_be() {
    let store = seeded(PAGES);
    let (filters, sort) = spec_and_sort(SortColumn::WordCount, SortDirection::Asc);
    let page = store.query_rows(&filters, &sort, 500, 25).unwrap();

    let expected = &sorted_by_word_count()[500..525];
    let got: Vec<u64> = page.rows.iter().map(|r| r.id as u64).collect();
    assert_eq!(got, expected);
    assert_eq!(page.total, PAGES);
    assert_eq!(page.offset, 500);
    assert_eq!(page.limit, 25);
}

#[test]
fn descending_is_the_same_slice_from_the_other_end() {
    let store = seeded(PAGES);
    let (filters, sort) = spec_and_sort(SortColumn::WordCount, SortDirection::Desc);
    let page = store.query_rows(&filters, &sort, 0, 10).unwrap();

    let mut expected = sorted_by_word_count();
    expected.reverse();
    let got: Vec<u64> = page.rows.iter().map(|r| r.id as u64).collect();
    assert_eq!(got, expected[..10]);
}

#[test]
fn consecutive_windows_do_not_overlap_or_skip() {
    // What the id tie-break is for. Without a total order, `OFFSET` can hand
    // the same row to two windows, or neither, as the user scrolls.
    let store = seeded(PAGES);
    let (filters, sort) = spec_and_sort(SortColumn::Status, SortDirection::Asc);

    let mut seen = Vec::new();
    for offset in (0..PAGES).step_by(200) {
        let page = store.query_rows(&filters, &sort, offset, 200).unwrap();
        seen.extend(page.rows.iter().map(|r| r.id));
    }
    let mut unique = seen.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(seen.len(), PAGES as usize);
    assert_eq!(
        unique.len(),
        PAGES as usize,
        "a row appeared in two windows"
    );
}

#[test]
fn the_limit_is_clamped_however_much_is_asked_for() {
    // The invariant resting on the caller's manners is not an invariant.
    let store = seeded(PAGES);
    let (filters, sort) = spec_and_sort(SortColumn::Url, SortDirection::Asc);
    let page = store.query_rows(&filters, &sort, 0, 100_000).unwrap();
    assert_eq!(page.limit, MAX_WINDOW);
    assert_eq!(page.rows.len(), MAX_WINDOW as usize);
    assert_eq!(page.total, PAGES, "the total is not clamped, the window is");
}

#[test]
fn the_total_counts_the_filter_not_the_table() {
    let store = seeded(PAGES);
    let filters = FilterSpec::new().with(Filter::Status(Comparison::Ge, 400));
    let sort = SortSpec::new(&filters, SortColumn::Status, SortDirection::Asc).unwrap();
    let page = store.query_rows(&filters, &sort, 0, 10).unwrap();

    let expected = (1..=PAGES).filter(|i| common::status_of(*i) >= 400).count() as u64;
    assert_eq!(page.total, expected);
    assert!(page.total < PAGES, "the filter must actually exclude rows");
    assert!(page.rows.iter().all(|r| r.status >= 400));
}

#[test]
fn an_offset_past_the_end_returns_no_rows_and_the_true_total() {
    // How the UI recovers a scroll position that a re-filter invalidated.
    let store = seeded(PAGES);
    let (filters, sort) = spec_and_sort(SortColumn::Url, SortDirection::Asc);
    let page = store.query_rows(&filters, &sort, PAGES * 10, 50).unwrap();
    assert!(page.rows.is_empty());
    assert_eq!(page.total, PAGES);
}

#[test]
fn a_row_view_carries_the_grid_columns_and_their_absences() {
    let store = seeded(PAGES);
    let filters = FilterSpec::new().with(Filter::Noindex(true));
    let sort = SortSpec::new(&filters, SortColumn::Url, SortDirection::Asc).unwrap();
    let page = store.query_rows(&filters, &sort, 0, 5).unwrap();

    let row = &page.rows[0];
    assert!(row.noindex);
    assert!(row.url.starts_with("http://e.com/"));
    assert_eq!(row.kind, "html");
    assert!(row.title.is_some(), "a title that exists comes back Some");
    assert_eq!(row.word_count, common::word_count_of(row.id as u64));
}

#[test]
fn a_reader_is_not_blocked_by_an_open_write_batch() {
    // WAL is what makes the grid usable *during* a crawl. Without this the app
    // stutters every time the writer commits — and the probe never tested it.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("live.pounce");
    let mut writing = Store::open(&path).unwrap();
    common::seed(&mut writing, 200);

    let reader = Store::open(&path).unwrap();
    let mut writer = Writer::with_batch_size(&mut writing, 10_000);
    // A failure row carries a foreign key to the frontier, because a real
    // crawl discovers a URL before it can be denied one.
    let blocked = pounce_core::CrawlUrl::parse("http://e.com/blocked").unwrap();
    writer.discover(&[(blocked.clone(), 1)]).unwrap();
    writer
        .fail(&blocked, "robots.txt disallows this path")
        .unwrap();
    // The batch is deliberately left open — this is mid-crawl, not after it.

    let (filters, sort) = spec_and_sort(SortColumn::Url, SortDirection::Asc);
    let page = reader.query_rows(&filters, &sort, 0, 50).unwrap();
    assert_eq!(page.total, 200, "the reader sees the committed rows");
    assert_eq!(page.rows.len(), 50);

    writer.flush().unwrap();
}

#[test]
fn a_filter_carrying_sql_returns_an_empty_window_not_an_error() {
    // The T3.1 injection case, once more through the real query rather than
    // through a hand-built count.
    let store = seeded(PAGES);
    let filters = FilterSpec::new().with(Filter::UrlContains("'; DROP TABLE pages; --".into()));
    let sort = SortSpec::new(&filters, SortColumn::Url, SortDirection::Asc).unwrap();
    let page = store.query_rows(&filters, &sort, 0, 50).unwrap();
    assert!(page.rows.is_empty());
    assert_eq!(page.total, 0);

    let (all, sort) = spec_and_sort(SortColumn::Url, SortDirection::Asc);
    assert_eq!(store.query_rows(&all, &sort, 0, 1).unwrap().total, PAGES);
}
