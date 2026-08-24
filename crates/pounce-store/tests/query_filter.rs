//! T3.1 — a `FilterSpec` compiles to parameterised SQL, and only that.
//!
//! Each variant is tested twice: once matching, once not. A filter that
//! matched everything would pass a one-sided test while being wrong.

mod common;

use common::{count_matching, seeded};
use pounce_parse::BodyKind;
use pounce_store::{Comparison, Filter, FilterSpec};

const PAGES: u64 = 300;

fn spec(filter: Filter) -> FilterSpec {
    FilterSpec::new().with(filter)
}

/// How many of the seeded pages satisfy `predicate` — the expectation computed
/// in Rust rather than restated as a number nobody can check.
fn expected(predicate: impl Fn(u64) -> bool) -> i64 {
    (1..=PAGES).filter(|i| predicate(*i)).count() as i64
}

#[test]
fn status_filters_both_ways() {
    let store = seeded(PAGES);
    assert_eq!(
        count_matching(&store, &spec(Filter::Status(Comparison::Eq, 404))),
        expected(|i| common::status_of(i) == 404)
    );
    assert_eq!(
        count_matching(&store, &spec(Filter::Status(Comparison::Ge, 400))),
        expected(|i| common::status_of(i) >= 400)
    );
    assert_eq!(
        count_matching(&store, &spec(Filter::Status(Comparison::Eq, 418))),
        0,
        "a status nothing was served with matches nothing"
    );
}

#[test]
fn depth_filters_both_ways() {
    let store = seeded(PAGES);
    assert_eq!(
        count_matching(&store, &spec(Filter::Depth(Comparison::Le, 1))),
        expected(|i| i % 5 <= 1)
    );
    assert_eq!(
        count_matching(&store, &spec(Filter::Depth(Comparison::Gt, 9))),
        0
    );
}

#[test]
fn word_count_filters_both_ways() {
    let store = seeded(PAGES);
    assert_eq!(
        count_matching(&store, &spec(Filter::WordCount(Comparison::Lt, 300))),
        expected(|i| common::word_count_of(i) < 300)
    );
    assert_eq!(
        count_matching(&store, &spec(Filter::WordCount(Comparison::Gt, 100_000))),
        0
    );
}

#[test]
fn kind_filters_both_ways() {
    let store = seeded(PAGES);
    assert_eq!(
        count_matching(&store, &spec(Filter::Kind(BodyKind::Pdf))),
        expected(|i| common::kind_of(i) == BodyKind::Pdf)
    );
    assert_eq!(
        count_matching(&store, &spec(Filter::Kind(BodyKind::Undeclared))),
        0,
        "every seeded page declared a type"
    );
}

#[test]
fn noindex_filters_both_ways() {
    let store = seeded(PAGES);
    assert_eq!(
        count_matching(&store, &spec(Filter::Noindex(true))),
        expected(|i| common::noindex_of(i))
    );
    assert_eq!(
        count_matching(&store, &spec(Filter::Noindex(false))),
        expected(|i| !common::noindex_of(i))
    );
}

#[test]
fn has_issue_filters_by_any_rule_and_by_one() {
    let store = seeded(PAGES);
    assert_eq!(
        count_matching(&store, &spec(Filter::HasIssue(None))),
        expected(|i| common::issue_of(i).is_some())
    );
    assert_eq!(
        count_matching(&store, &spec(Filter::HasIssue(Some("title.missing")))),
        expected(|i| common::issue_of(i).map(|(r, _)| r) == Some("title.missing"))
    );
    assert_eq!(
        count_matching(&store, &spec(Filter::HasIssue(Some("media.broken-image")))),
        0,
        "a real rule id that nothing triggered matches nothing"
    );
}

#[test]
fn url_contains_matches_a_substring_and_nothing_else() {
    let store = seeded(PAGES);
    assert_eq!(
        count_matching(&store, &spec(Filter::UrlContains("/blog/".into()))),
        expected(|i| i % 5 == 0)
    );
    assert_eq!(
        count_matching(&store, &spec(Filter::UrlContains("/shop/".into()))),
        0
    );
}

#[test]
fn filters_and_together() {
    let store = seeded(PAGES);
    let spec = FilterSpec::new()
        .with(Filter::Status(Comparison::Eq, 200))
        .with(Filter::Noindex(false));
    assert_eq!(
        count_matching(&store, &spec),
        expected(|i| common::status_of(i) == 200 && !common::noindex_of(i))
    );
}

#[test]
fn an_empty_spec_compiles_to_no_where_clause() {
    // Not `WHERE 1=1`: an unfiltered grid should get the plan for a bare scan.
    let (sql, params) = FilterSpec::new().compile();
    assert!(sql.is_empty(), "{sql}");
    assert!(params.is_empty());
    assert_eq!(
        count_matching(&seeded(PAGES), &FilterSpec::new()),
        PAGES as i64
    );
}

// ---- the injection surface -----------------------------------------------

#[test]
fn a_filter_carrying_sql_is_data_and_leaves_the_table_standing() {
    let store = seeded(PAGES);
    let attack = "'; DROP TABLE pages; --";
    assert_eq!(
        count_matching(&store, &spec(Filter::UrlContains(attack.into()))),
        0,
        "no seeded URL contains that text"
    );
    assert_eq!(
        common::count_matching(&store, &FilterSpec::new()),
        PAGES as i64,
        "the table must still be there, with every row"
    );
}

#[test]
fn the_compiled_sql_contains_none_of_the_users_bytes() {
    // Asserted against the parameter list rather than by eyeballing the SQL:
    // the value must be *in* the parameters, and the SQL must be the same
    // string whatever the user typed.
    let needle = "'; DROP TABLE pages; --";
    let (sql, params) = spec(Filter::UrlContains(needle.into())).compile();
    assert!(!sql.contains(needle), "the needle reached the SQL: {sql}");
    assert!(
        !sql.contains('%'),
        "the wildcards belong to the value: {sql}"
    );
    assert_eq!(params.len(), 1);
    assert_eq!(
        params[0],
        rusqlite::types::Value::Text(format!("%{needle}%"))
    );

    let (other, _) = spec(Filter::UrlContains("something else".into())).compile();
    assert_eq!(sql, other, "the SQL must not vary with the user's input");
}

#[test]
fn every_variant_binds_exactly_the_parameters_it_names() {
    // A fragment whose `?` count and parameter count disagree is either an
    // interpolation or a bind error, and both are the failure this task exists
    // to prevent.
    let cases = [
        Filter::Status(Comparison::Eq, 200),
        Filter::Depth(Comparison::Le, 3),
        Filter::WordCount(Comparison::Gt, 100),
        Filter::Kind(BodyKind::Html),
        Filter::Noindex(true),
        Filter::HasIssue(None),
        Filter::HasIssue(Some("title.missing")),
        Filter::UrlContains("blog".into()),
    ];
    for filter in cases {
        let (sql, params) = spec(filter.clone()).compile();
        assert_eq!(
            sql.matches('?').count(),
            params.len(),
            "{filter:?} binds {} values into {sql}",
            params.len()
        );
    }
}
