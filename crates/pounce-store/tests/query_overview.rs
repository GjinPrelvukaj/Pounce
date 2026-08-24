//! T3.4 — the issue overview's aggregates.
//!
//! Small queries, one trap: joining `pages`. That would cost a lookup per issue
//! for columns the overview never shows, and would drop every finding whose
//! subject never became a page — a redirect loop, an unreachable host — which
//! are the findings a report most needs to carry.

mod common;

use common::seeded;
use pounce_store::{Store, Writer};

const PAGES: u64 = 600;

#[test]
fn every_rule_that_fired_is_counted_once_with_its_severity() {
    let store = seeded(PAGES);
    let overview = store.issue_overview().unwrap();

    let expect_missing_title = (1..=PAGES)
        .filter(|i| common::issue_of(*i).map(|(r, _)| r) == Some("title.missing"))
        .count() as u64;
    let expect_missing_desc = (1..=PAGES)
        .filter(|i| common::issue_of(*i).map(|(r, _)| r) == Some("description.missing"))
        .count() as u64;

    let title = overview
        .by_rule
        .iter()
        .find(|c| c.rule_id == "title.missing")
        .expect("title.missing fired on the seeded crawl");
    assert_eq!(title.issues, expect_missing_title);
    assert_eq!(title.urls, expect_missing_title, "one issue per page here");
    assert_eq!(title.severity, "critical");

    assert_eq!(
        overview.by_rule.len(),
        2,
        "only the rules that fired appear"
    );
    assert_eq!(
        overview.total_issues,
        expect_missing_title + expect_missing_desc
    );
    assert_eq!(overview.urls_with_issues, overview.total_issues);
}

#[test]
fn the_worst_rule_comes_first() {
    // The overview is read top-down; an arbitrary order would put the site's
    // biggest problem anywhere on the screen.
    let store = seeded(PAGES);
    let overview = store.issue_overview().unwrap();
    let counts: Vec<u64> = overview.by_rule.iter().map(|c| c.issues).collect();
    let mut sorted = counts.clone();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    assert_eq!(counts, sorted);
}

#[test]
fn severity_totals_add_up_to_the_issue_total() {
    let store = seeded(PAGES);
    let overview = store.issue_overview().unwrap();
    let summed: u64 = overview.by_severity.iter().map(|(_, n)| n).sum();
    assert_eq!(summed, overview.total_issues);
    assert!(
        overview.by_severity.iter().any(|(s, _)| s == "critical"),
        "{:?}",
        overview.by_severity
    );
}

#[test]
fn several_issues_on_one_page_are_several_issues_but_one_url() {
    // A template with two oversized images is one bad page, not a site-wide
    // problem, and the overview has to be able to say which.
    let mut store = Store::in_memory().unwrap();
    common::seed(&mut store, 10);
    let url = common::url_of(1);
    {
        let mut writer = Writer::with_batch_size(&mut store, 100);
        let page_id = writer.page_id(&url).unwrap();
        writer
            .issues(
                &url,
                page_id,
                &[
                    ("media.oversized-image", "warning", Some("hero.jpg")),
                    ("media.oversized-image", "warning", Some("banner.jpg")),
                ],
            )
            .unwrap();
        writer.flush().unwrap();
    }

    let counted = store
        .issue_overview()
        .unwrap()
        .by_rule
        .into_iter()
        .find(|c| c.rule_id == "media.oversized-image")
        .unwrap();
    assert_eq!(counted.issues, 2);
    assert_eq!(counted.urls, 1);
}

#[test]
fn a_finding_about_a_url_that_never_became_a_page_still_counts() {
    // The join that would have been convenient is the one that drops these.
    let mut store = Store::in_memory().unwrap();
    common::seed(&mut store, 10);
    {
        let mut writer = Writer::with_batch_size(&mut store, 100);
        writer
            .issues(
                "http://e.com/loop",
                None,
                &[("response.redirect-loop", "critical", None)],
            )
            .unwrap();
        writer.flush().unwrap();
    }

    let before = pageless_free_urls(&store);
    let overview = store.issue_overview().unwrap();
    assert!(
        overview
            .by_rule
            .iter()
            .any(|c| c.rule_id == "response.redirect-loop" && c.issues == 1),
        "a pageless finding was dropped: {:?}",
        overview.by_rule
    );
    // The headline count is two queries — distinct page ids, plus the findings
    // with no page — because `count(DISTINCT)` ignores NULLs. Without the
    // second, a crawl's redirect loops would be missing from its own total.
    assert_eq!(
        overview.urls_with_issues,
        before + 1,
        "the pageless URL is not in urls_with_issues"
    );
}

/// URLs with issues that *do* have a page row.
fn pageless_free_urls(store: &Store) -> u64 {
    store
        .conn()
        .query_row(
            "SELECT count(DISTINCT url) FROM issues WHERE page_id IS NOT NULL",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap() as u64
}

#[test]
fn the_grouped_query_is_served_by_an_index_not_a_temp_b_tree() {
    // 472 ms at 1M issues without this index, 102 ms with it. `GROUP BY` on an
    // unindexed pair sorts the whole table first.
    let store = seeded(PAGES);
    let plans: Vec<String> = store
        .conn()
        .prepare(
            "EXPLAIN QUERY PLAN SELECT rule_id, severity, count(*), count(DISTINCT url) \
             FROM issues GROUP BY rule_id, severity ORDER BY count(*) DESC, rule_id ASC",
        )
        .unwrap()
        .query_map([], |r| r.get(3))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(
        !plans.iter().any(|p| p.contains("TEMP B-TREE FOR GROUP BY")),
        "the overview's GROUP BY is sorting the whole issues table: {plans:?}"
    );
}

#[test]
fn the_overview_does_not_touch_pages() {
    let store = seeded(PAGES);
    for sql in [
        "SELECT rule_id, severity, count(*), count(DISTINCT url) FROM issues \
         GROUP BY rule_id, severity ORDER BY count(*) DESC, rule_id ASC",
        "SELECT count(*), count(DISTINCT page_id) FROM issues",
        "SELECT count(DISTINCT url) FROM issues WHERE page_id IS NULL",
    ] {
        let plans: Vec<String> = store
            .conn()
            .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
            .unwrap()
            .query_map([], |r| r.get(3))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(
            !plans.iter().any(|p| p.contains("pages")),
            "the overview joined pages: {plans:?}"
        );
    }
}

#[test]
fn a_crawl_with_no_issues_reports_zero_rather_than_failing() {
    let store = Store::in_memory().unwrap();
    let overview = store.issue_overview().unwrap();
    assert_eq!(overview.total_issues, 0);
    assert_eq!(overview.urls_with_issues, 0);
    assert!(overview.by_rule.is_empty());
    assert!(overview.by_severity.is_empty());
}
