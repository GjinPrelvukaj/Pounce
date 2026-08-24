//! The wire shapes for M3's query API, and the validation that happens on the
//! way in.
//!
//! Everything the UI sends arrives as JSON, so the types M3 built cannot cross
//! this boundary directly — and one of them must not. `Filter::HasIssue` holds
//! a `&'static str` from the rule registry precisely so a filter cannot name a
//! rule that does not exist; a string off the wire has no such guarantee. This
//! module is where the guarantee is restored: an unknown rule id is refused
//! here, rather than becoming a query that quietly matches nothing.
//!
//! The other reason this is not just `#[derive(Deserialize)]` on M3's enums: a
//! `SortSpec` is only constructible through `SortSpec::new`, which refuses
//! filter/sort pairs no index can serve. Deriving `Deserialize` on it would
//! hand callers a way to build one that skipped that check — the same mistake
//! `CrawlUrl`'s hand-written serde exists to prevent.

use pounce_audit::Registry;
use pounce_parse::BodyKind;
use pounce_store::{
    Comparison, Filter, FilterSpec, IssueOverview, Page, SortColumn, SortDirection, SortSpec, Store,
};
use std::collections::HashSet;

/// What went wrong, in a shape the UI can branch on rather than string-match.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ApiError {
    /// No `.pounce` file is open. The grid should be showing its empty state,
    /// not an error toast.
    NoCrawlOpen,
    /// The rule id in a `hasIssue` filter is not one this build has.
    UnknownRule { rule: String },
    /// `SortSpec::new` refused the pair. Carries both halves so the UI can say
    /// which sort it disabled and why.
    UnsupportedPair { filter: String, sort: String },
    /// The seed URL did not parse, or the crawl failed.
    Crawl { message: String },
    /// Anything the store itself returned.
    Store { message: String },
}

impl From<pounce_store::StoreError> for ApiError {
    fn from(e: pounce_store::StoreError) -> Self {
        ApiError::Store {
            message: e.to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ComparisonDto {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl From<ComparisonDto> for Comparison {
    fn from(c: ComparisonDto) -> Self {
        match c {
            ComparisonDto::Eq => Comparison::Eq,
            ComparisonDto::Ne => Comparison::Ne,
            ComparisonDto::Lt => Comparison::Lt,
            ComparisonDto::Le => Comparison::Le,
            ComparisonDto::Gt => Comparison::Gt,
            ComparisonDto::Ge => Comparison::Ge,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BodyKindDto {
    Html,
    Pdf,
    Image,
    Other,
    Undeclared,
}

impl From<BodyKindDto> for BodyKind {
    fn from(k: BodyKindDto) -> Self {
        match k {
            BodyKindDto::Html => BodyKind::Html,
            BodyKindDto::Pdf => BodyKind::Pdf,
            BodyKindDto::Image => BodyKind::Image,
            BodyKindDto::Other => BodyKind::Other,
            BodyKindDto::Undeclared => BodyKind::Undeclared,
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(tag = "field", rename_all = "camelCase")]
pub enum FilterDto {
    Status {
        cmp: ComparisonDto,
        value: u16,
    },
    Depth {
        cmp: ComparisonDto,
        value: u16,
    },
    WordCount {
        cmp: ComparisonDto,
        value: u32,
    },
    Kind {
        value: BodyKindDto,
    },
    Noindex {
        value: bool,
    },
    /// `rule: null` is "any issue at all".
    HasIssue {
        rule: Option<String>,
    },
    UrlContains {
        needle: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SortColumnDto {
    Url,
    Status,
    Depth,
    Size,
    WordCount,
    ElapsedMs,
    Title,
}

impl From<SortColumnDto> for SortColumn {
    fn from(c: SortColumnDto) -> Self {
        match c {
            SortColumnDto::Url => SortColumn::Url,
            SortColumnDto::Status => SortColumn::Status,
            SortColumnDto::Depth => SortColumn::Depth,
            SortColumnDto::Size => SortColumn::Size,
            SortColumnDto::WordCount => SortColumn::WordCount,
            SortColumnDto::ElapsedMs => SortColumn::ElapsedMs,
            SortColumnDto::Title => SortColumn::Title,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SortDirectionDto {
    Asc,
    Desc,
}

impl From<SortDirectionDto> for SortDirection {
    fn from(d: SortDirectionDto) -> Self {
        match d {
            SortDirectionDto::Asc => SortDirection::Asc,
            SortDirectionDto::Desc => SortDirection::Desc,
        }
    }
}

/// Every rule id this build registered, as the `&'static str`s the registry
/// owns. Built once per call rather than kept in state: it is 30 pointers, and
/// a stale copy of it would be a filter that silently stops working after a
/// rule is renamed.
fn known_rules(registry: &Registry) -> HashSet<&'static str> {
    registry
        .page_rules()
        .iter()
        .map(|r| r.meta().id)
        .chain(registry.site_rules().iter().map(|r| r.meta().id))
        .collect()
}

/// Turns a rule id off the wire back into the registry's own `&'static str`.
///
/// This lookup *is* the guarantee. Returning the borrowed static — not the
/// caller's `String` — is what keeps `Filter::HasIssue` unable to name a rule
/// that does not exist.
fn resolve_rule(registry: &Registry, rule: &str) -> Result<&'static str, ApiError> {
    known_rules(registry)
        .into_iter()
        .find(|known| *known == rule)
        .ok_or_else(|| ApiError::UnknownRule {
            rule: rule.to_string(),
        })
}

pub fn to_filter(registry: &Registry, dto: &FilterDto) -> Result<Filter, ApiError> {
    Ok(match dto {
        FilterDto::Status { cmp, value } => Filter::Status((*cmp).into(), *value),
        FilterDto::Depth { cmp, value } => Filter::Depth((*cmp).into(), *value),
        FilterDto::WordCount { cmp, value } => Filter::WordCount((*cmp).into(), *value),
        FilterDto::Kind { value } => Filter::Kind((*value).into()),
        FilterDto::Noindex { value } => Filter::Noindex(*value),
        FilterDto::HasIssue { rule: None } => Filter::HasIssue(None),
        FilterDto::HasIssue { rule: Some(rule) } => {
            Filter::HasIssue(Some(resolve_rule(registry, rule)?))
        }
        FilterDto::UrlContains { needle } => Filter::UrlContains(needle.clone()),
    })
}

pub fn to_spec(registry: &Registry, dtos: &[FilterDto]) -> Result<FilterSpec, ApiError> {
    let mut spec = FilterSpec::new();
    for dto in dtos {
        spec = spec.with(to_filter(registry, dto)?);
    }
    Ok(spec)
}

/// One window of the grid.
///
/// The clamp on `limit` lives in `Store::query_rows`, not here — a second one
/// at the boundary would be a number to keep in step with the first.
pub fn rows(
    store: &Store,
    registry: &Registry,
    filters: &[FilterDto],
    sort: SortColumnDto,
    direction: SortDirectionDto,
    offset: u64,
    limit: u32,
) -> Result<Page, ApiError> {
    let spec = to_spec(registry, filters)?;
    let sort = SortSpec::new(&spec, sort.into(), direction.into()).map_err(|e| match e {
        pounce_store::QueryError::UnsupportedPair { filter, sort } => ApiError::UnsupportedPair {
            filter: filter.to_string(),
            sort: sort.to_string(),
        },
    })?;
    Ok(store.query_rows(&spec, &sort, offset, limit)?)
}

pub fn overview(store: &Store) -> Result<IssueOverview, ApiError> {
    Ok(store.issue_overview()?)
}

/// Which `(filter, sort)` pairs this build will run, so the UI can grey out a
/// sort rather than offer it and take an error.
///
/// Derived from the same `is_supported` the store enforces — asking the engine
/// beats keeping a second list in TypeScript that drifts.
pub fn supported_sorts(
    registry: &Registry,
    filters: &[FilterDto],
) -> Result<Vec<SortColumnDto>, ApiError> {
    let spec = to_spec(registry, filters)?;
    Ok([
        SortColumnDto::Url,
        SortColumnDto::Status,
        SortColumnDto::Depth,
        SortColumnDto::Size,
        SortColumnDto::WordCount,
        SortColumnDto::ElapsedMs,
        SortColumnDto::Title,
    ]
    .into_iter()
    .filter(|c| SortSpec::new(&spec, (*c).into(), SortDirection::Asc).is_ok())
    .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> Registry {
        let mut r = Registry::new();
        pounce_audit::register_all(&mut r).unwrap();
        r
    }

    #[test]
    fn a_known_rule_id_becomes_the_registrys_own_static_str() {
        let r = registry();
        let id = known_rules(&r).into_iter().next().unwrap();
        let filter = to_filter(
            &r,
            &FilterDto::HasIssue {
                rule: Some(id.to_string()),
            },
        )
        .unwrap();
        assert_eq!(filter, Filter::HasIssue(Some(id)));
    }

    #[test]
    fn an_unknown_rule_id_is_refused_at_the_boundary() {
        // Without this the filter would compile to SQL that matches nothing,
        // and an empty grid looks exactly like an answer.
        let err = to_filter(
            &registry(),
            &FilterDto::HasIssue {
                rule: Some("title.definitely-not-a-rule".into()),
            },
        )
        .unwrap_err();
        assert_eq!(
            err,
            ApiError::UnknownRule {
                rule: "title.definitely-not-a-rule".into()
            }
        );
    }

    #[test]
    fn every_filter_variant_survives_the_wire_format() {
        let json = r#"[
            {"field":"status","cmp":"eq","value":200},
            {"field":"depth","cmp":"le","value":2},
            {"field":"wordCount","cmp":"lt","value":300},
            {"field":"kind","value":"html"},
            {"field":"noindex","value":false},
            {"field":"hasIssue","rule":null},
            {"field":"urlContains","needle":"/blog/"}
        ]"#;
        let dtos: Vec<FilterDto> = serde_json::from_str(json).unwrap();
        assert_eq!(dtos.len(), 7);
        let spec = to_spec(&registry(), &dtos).unwrap();
        assert_eq!(spec.filters().len(), 7);
    }

    #[test]
    fn an_unsupported_pair_comes_back_named_rather_than_as_a_slow_query() {
        let err = rows(
            &Store::in_memory().unwrap(),
            &registry(),
            &[FilterDto::UrlContains {
                needle: "blog".into(),
            }],
            SortColumnDto::WordCount,
            SortDirectionDto::Asc,
            0,
            50,
        )
        .unwrap_err();
        assert_eq!(
            err,
            ApiError::UnsupportedPair {
                filter: "url_contains".into(),
                sort: "word_count".into()
            }
        );
    }

    #[test]
    fn the_supported_sorts_shrink_when_the_filter_needs_them_to() {
        let r = registry();
        let all = supported_sorts(&r, &[]).unwrap();
        assert_eq!(all.len(), 7, "an unfiltered grid can sort by anything");

        let substring = supported_sorts(
            &r,
            &[FilterDto::UrlContains {
                needle: "blog".into(),
            }],
        )
        .unwrap();
        assert_eq!(substring, vec![SortColumnDto::Url]);

        // A range filter keeps the cheap sort columns and loses the two that
        // measured ~450 ms at 1M.
        let range = supported_sorts(
            &r,
            &[FilterDto::Depth {
                cmp: ComparisonDto::Le,
                value: 2,
            }],
        )
        .unwrap();
        assert!(range.contains(&SortColumnDto::Url));
        assert!(!range.contains(&SortColumnDto::WordCount));
    }

    /// A store with a handful of real pages, written through the same `Writer`
    /// a crawl uses.
    fn seeded(pages: u64) -> Store {
        use pounce_core::CrawlUrl;
        use pounce_parse::{MetaRobots, PageRecord};
        use pounce_store::Writer;

        let mut store = Store::in_memory().unwrap();
        {
            let mut writer = Writer::new(&mut store);
            for i in 1..=pages {
                let url = format!("http://e.com/page-{i}");
                let record = PageRecord {
                    url: CrawlUrl::parse(&url).unwrap(),
                    status: if i % 5 == 0 { 404 } else { 200 },
                    depth: 1,
                    size: 1_000,
                    truncated: false,
                    content_type: Some("text/html".into()),
                    charset: Some("utf-8".into()),
                    kind: BodyKind::Html,
                    content_type_mismatch: false,
                    elapsed_ms: 5,
                    time_to_headers_ms: 2,
                    redirect_chain: vec![],
                    title: Some(format!("Page {i}")),
                    title_count: 1,
                    meta_description: None,
                    h1: vec![],
                    h2: vec![],
                    canonical: None,
                    canonical_url: None,
                    meta_robots: MetaRobots::default(),
                    hreflang: vec![],
                    open_graph: vec![],
                    links: vec![],
                    images: vec![],
                    word_count: (100 + i) as u32,
                    body_hash: None,
                };
                let id = writer.push(&record).unwrap();
                if i % 5 == 0 {
                    writer
                        .issues(
                            &url,
                            Some(id),
                            &[("response.client-error", "critical", None)],
                        )
                        .unwrap();
                }
            }
            writer.flush().unwrap();
        }
        store.build_query_indices().unwrap();
        store
    }

    #[test]
    fn a_window_of_rows_comes_back_through_the_wire_types() {
        let store = seeded(40);
        let page = rows(
            &store,
            &registry(),
            &[FilterDto::Status {
                cmp: ComparisonDto::Eq,
                value: 404,
            }],
            SortColumnDto::Url,
            SortDirectionDto::Asc,
            0,
            10,
        )
        .unwrap();

        assert_eq!(page.total, 8, "every fifth page of forty");
        assert!(page.rows.iter().all(|r| r.status == 404));
        // The projection is the grid's nine columns, and it survives JSON.
        let json = serde_json::to_value(&page).unwrap();
        assert!(json["rows"][0]["wordCount"].is_number());
        assert!(json["rows"][0]["url"].as_str().unwrap().starts_with("http"));
    }

    #[test]
    fn the_issue_filter_reaches_the_denormalised_flag() {
        let store = seeded(40);
        let page = rows(
            &store,
            &registry(),
            &[FilterDto::HasIssue { rule: None }],
            SortColumnDto::WordCount,
            SortDirectionDto::Desc,
            0,
            10,
        )
        .unwrap();
        assert_eq!(page.total, 8);

        let overview = overview(&store).unwrap();
        assert_eq!(overview.total_issues, 8);
        assert_eq!(overview.urls_with_issues, 8);
    }

    #[test]
    fn an_empty_store_answers_with_an_empty_window_not_an_error() {
        let store = Store::in_memory().unwrap();
        let page = rows(
            &store,
            &registry(),
            &[],
            SortColumnDto::Url,
            SortDirectionDto::Asc,
            0,
            50,
        )
        .unwrap();
        assert_eq!(page.total, 0);
        assert!(page.rows.is_empty());
        assert_eq!(overview(&store).unwrap().total_issues, 0);
    }
}
