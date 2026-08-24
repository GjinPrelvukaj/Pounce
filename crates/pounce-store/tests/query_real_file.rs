//! The query layer against a **real crawl**, not a seeded store.
//!
//! Everything else in this crate builds its own rows, which means every test
//! shares the seeder's assumptions. This one opens a `.pounce` file a crawl
//! actually produced, so a shape only the crawler creates — a redirect chain,
//! an untitled page, a resource URL — reaches the query layer at least once.
//!
//! ```text
//! ./target/release/fixture-site --pages 5000 --seed 42 --port 8087 &
//! ./target/release/pounce crawl http://localhost:8087/ --output /tmp/e2e.pounce
//! POUNCE_FILE=/tmp/e2e.pounce cargo test -p pounce-store --test query_real_file -- --ignored --nocapture
//! ```

use pounce_store::{Comparison, Filter, FilterSpec, SortColumn, SortDirection, SortSpec, Store};

#[test]
#[ignore = "needs POUNCE_FILE pointing at a crawled .pounce"]
fn a_crawled_file_answers_every_supported_query() {
    let Ok(path) = std::env::var("POUNCE_FILE") else {
        panic!("set POUNCE_FILE to a crawled .pounce file");
    };
    let store = Store::open(&path).unwrap();

    let filters = FilterSpec::new();
    let sort = SortSpec::new(&filters, SortColumn::Url, SortDirection::Asc).unwrap();
    let all = store.query_rows(&filters, &sort, 0, 10).unwrap();
    eprintln!("{} pages in {path}", all.total);
    assert!(all.total > 0, "the file has no pages");
    assert_eq!(all.rows.len(), 10);

    // Every filter kind, every sort column it is allowed with, over real rows.
    let candidates = [
        ("status=200", Filter::Status(Comparison::Eq, 200)),
        ("status>=400", Filter::Status(Comparison::Ge, 400)),
        ("kind=html", Filter::Kind(pounce_parse::BodyKind::Html)),
        ("noindex", Filter::Noindex(true)),
        ("depth<=2", Filter::Depth(Comparison::Le, 2)),
        ("has_issue", Filter::HasIssue(None)),
        ("word_count>100", Filter::WordCount(Comparison::Gt, 100)),
        ("url~/section/", Filter::UrlContains("/section/".into())),
    ];
    let mut offered = 0;
    for (name, filter) in candidates {
        let filters = FilterSpec::new().with(filter);
        for &column in SortColumn::all() {
            let Ok(sort) = SortSpec::new(&filters, column, SortDirection::Desc) else {
                continue;
            };
            offered += 1;
            let page = store.query_rows(&filters, &sort, 0, 50).unwrap();
            // The window must be consistent with the total it reports.
            assert!(
                page.rows.len() as u64 <= page.total,
                "{name} by {} returned more rows than it counted",
                column.column()
            );
            if page.total > 0 {
                assert!(
                    !page.rows.is_empty(),
                    "{name} counted rows but returned none"
                );
            }
        }
    }
    eprintln!("{offered} filter x sort pairs answered");

    let overview = store.issue_overview().unwrap();
    eprintln!(
        "{} rules fired, {} issues over {} URLs",
        overview.by_rule.len(),
        overview.total_issues,
        overview.urls_with_issues
    );
    assert!(overview.total_issues > 0, "a real crawl found no issues");
    assert!(overview.urls_with_issues <= all.total + overview.total_issues);
    let summed: u64 = overview.by_severity.iter().map(|(_, n)| n).sum();
    assert_eq!(summed, overview.total_issues);

    // A rule's own filter must agree with the overview's count for it.
    let (worst, _) = (&overview.by_rule[0].rule_id, ());
    let rule: &'static str = Box::leak(worst.clone().into_boxed_str());
    let filters = FilterSpec::new().with(Filter::HasIssue(Some(rule)));
    let sort = SortSpec::new(&filters, SortColumn::Url, SortDirection::Asc).unwrap();
    let by_filter = store.query_rows(&filters, &sort, 0, 1).unwrap().total;
    assert_eq!(
        by_filter, overview.by_rule[0].urls,
        "the grid filter and the overview disagree about {rule}"
    );
    eprintln!("{rule}: {by_filter} pages by filter, matching the overview");
}
