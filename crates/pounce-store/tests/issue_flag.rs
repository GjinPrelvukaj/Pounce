//! `pages.has_issue` is a cache of a fact `issues` still owns.
//!
//! That is the whole argument for adding it (migration 013), so it has to be
//! checkable: every test here compares the flag against the table it was
//! derived from rather than against a remembered number.

mod common;

use common::seeded;
use pounce_store::{Filter, FilterSpec, SortColumn, SortDirection, SortSpec, Store, Writer};

const PAGES: u64 = 400;

/// Pages whose flag disagrees with `issues`. Must always be zero.
fn disagreements(store: &Store) -> i64 {
    store
        .conn()
        .query_row(
            "SELECT count(*) FROM pages p WHERE p.has_issue <> \
             (EXISTS (SELECT 1 FROM issues i WHERE i.page_id = p.id))",
            [],
            |r| r.get(0),
        )
        .unwrap()
}

#[test]
fn the_flag_agrees_with_the_issues_table() {
    let store = seeded(PAGES);
    assert_eq!(disagreements(&store), 0);
}

#[test]
fn the_flag_is_current_during_a_crawl_not_only_after_it() {
    // The grid is usable while the crawl runs, so a filter that only became
    // true at `build_query_indices` would read as "no problems yet" for the
    // whole crawl — the most misleading answer available.
    let mut store = Store::in_memory().unwrap();
    common::seed_pages_only(&mut store, 100);
    assert_eq!(disagreements(&store), 0, "the writer left the flag stale");
}

#[test]
fn the_filter_and_the_table_count_the_same_pages() {
    let store = seeded(PAGES);
    let filters = FilterSpec::new().with(Filter::HasIssue(None));
    let sort = SortSpec::new(&filters, SortColumn::Url, SortDirection::Asc).unwrap();
    let by_filter = store.query_rows(&filters, &sort, 0, 1).unwrap().total;

    let by_table: i64 = store
        .conn()
        .query_row(
            "SELECT count(DISTINCT page_id) FROM issues WHERE page_id IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(by_filter, by_table as u64);
    assert!(by_filter > 0);
}

#[test]
fn an_older_file_has_its_flags_rebuilt_rather_than_left_at_zero() {
    // Migration 013 defaults the column to 0, so every page in a file written
    // before it starts out claiming no findings. The repair pass in
    // `build_query_indices` is what makes the column true rather than new.
    let mut store = Store::in_memory().unwrap();
    common::seed(&mut store, 50);
    store
        .conn()
        .execute("UPDATE pages SET has_issue = 0", [])
        .unwrap();
    assert!(disagreements(&store) > 0, "the test must start out wrong");

    store.build_query_indices().unwrap();
    assert_eq!(disagreements(&store), 0, "the repair pass did not run");
}

#[test]
fn a_finding_with_no_page_flags_nothing_and_is_still_counted() {
    // A redirect loop has no page row to flag. It must not be lost: the
    // overview counts it through `issues`, which is still the source of truth.
    let mut store = Store::in_memory().unwrap();
    common::seed(&mut store, 20);
    {
        let mut writer = Writer::with_batch_size(&mut store, 10);
        writer
            .issues(
                "http://e.com/loop",
                None,
                &[("response.redirect-loop", "critical", None)],
            )
            .unwrap();
        writer.flush().unwrap();
    }
    assert_eq!(disagreements(&store), 0);
    assert!(
        store
            .issue_overview()
            .unwrap()
            .by_rule
            .iter()
            .any(|c| c.rule_id == "response.redirect-loop")
    );
}

#[test]
fn re_running_the_rules_does_not_double_flag_or_unflag() {
    // Issues are appended, not replaced, and a page keeps its flag.
    let mut store = Store::in_memory().unwrap();
    common::seed(&mut store, 30);
    let url = common::url_of(2);
    {
        let mut writer = Writer::with_batch_size(&mut store, 10);
        let page_id = writer.page_id(&url).unwrap();
        writer
            .issues(&url, page_id, &[("title.too-long", "warning", None)])
            .unwrap();
        writer
            .issues(&url, page_id, &[("title.too-long", "warning", None)])
            .unwrap();
        writer.flush().unwrap();
    }
    assert_eq!(disagreements(&store), 0);
    let flagged: i64 = store
        .conn()
        .query_row("SELECT has_issue FROM pages WHERE url = ?1", [&url], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(flagged, 1);
}
