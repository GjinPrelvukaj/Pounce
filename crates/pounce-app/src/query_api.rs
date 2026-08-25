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

/// Whether a new crawl may start, given whatever the last one left behind.
///
/// Two crawls into one file is not merely untidy: both check the output path
/// before either creates it, both then run the migrations, and the loser gets
/// "table pages already exists" — with two writers on one database from then
/// on. Refusing here is cheaper than reasoning about that.
pub fn may_start(running: Option<pounce_core::CrawlStatus>) -> Result<(), ApiError> {
    match running {
        Some(status) if !status.is_terminal() => Err(ApiError::Crawl {
            message: "a crawl is already running".into(),
        }),
        _ => Ok(()),
    }
}

/// What the new-crawl screen sends.
///
/// Every field is optional-with-a-default rather than required, so the screen
/// can offer "just crawl this" without the user meeting a form first.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrawlSettings {
    pub seed: String,
    pub output: String,
    #[serde(default)]
    pub images: bool,
    pub max_depth: Option<u16>,
    pub max_urls: Option<u64>,
    pub max_duration_secs: Option<u64>,
    /// Simultaneous requests to one host. The politeness knob users reach for
    /// first, and the one that gets them blocked.
    pub per_host_concurrency: Option<usize>,
    /// Spacing between requests to a host whose robots.txt names no
    /// `Crawl-delay`.
    pub delay_ms: Option<u64>,
}

/// The ceiling on per-host concurrency this build will accept.
///
/// Not a preference. Politeness defaults are correctness here — a crawler that
/// gets its user IP-banned is a liability — so the screen may raise
/// concurrency, and may not raise it to something a small site experiences as
/// an outage.
pub const MAX_PER_HOST_CONCURRENCY: usize = 16;

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
    /// The output path is taken. Carries a free name beside it, because
    /// "output already exists" is accurate and leaves the user to invent the
    /// next filename themselves — which is the moment they discover the form
    /// they just filled in is still there.
    OutputExists { path: String, suggestion: String },
    /// The seed URL did not parse. `suggestion` is a corrected form when the
    /// input is one obvious fix away — almost always a missing scheme.
    BadSeed {
        input: String,
        message: String,
        suggestion: Option<String>,
    },
    /// The crawl failed.
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

/// Splits settings into the two things the engine takes: the limits that ride
/// on the lifecycle, and the fetch configuration that is the politeness half.
///
/// Rejects rather than clamps. A screen that asked for 64 concurrent requests
/// and silently got 16 would report a crawl it did not run.
pub fn to_engine(
    settings: &CrawlSettings,
) -> Result<(pounce_core::CrawlLimits, pounce_http::fetch::FetchConfig), ApiError> {
    let defaults = pounce_http::fetch::FetchConfig::default();
    let concurrency = settings
        .per_host_concurrency
        .unwrap_or(defaults.max_concurrent_per_host);
    if concurrency == 0 || concurrency > MAX_PER_HOST_CONCURRENCY {
        return Err(ApiError::Crawl {
            message: format!(
                "per-host concurrency must be between 1 and {MAX_PER_HOST_CONCURRENCY}"
            ),
        });
    }
    if settings.max_urls == Some(0) || settings.max_duration_secs == Some(0) {
        return Err(ApiError::Crawl {
            message: "a limit of zero would crawl nothing; leave it unset instead".into(),
        });
    }

    Ok((
        pounce_core::CrawlLimits {
            max_depth: settings.max_depth,
            max_urls: settings.max_urls,
            max_duration: settings
                .max_duration_secs
                .map(std::time::Duration::from_secs),
        },
        pounce_http::fetch::FetchConfig {
            max_concurrent_per_host: concurrency,
            default_delay: settings
                .delay_ms
                .map(std::time::Duration::from_millis)
                .unwrap_or(defaults.default_delay),
            ..defaults
        },
    ))
}

/// A path like the one asked for that nothing is using yet.
///
/// Counts up rather than stamping a time: `crawl-2.pounce` is a name someone
/// would have chosen, and `crawl-20260825T041233.pounce` is one they have to
/// read character by character to tell from its neighbour. Gives up after 99
/// and returns the original, which the caller reports as taken — a directory
/// with a hundred numbered crawls is not a case worth a cleverer scheme.
pub fn free_name(path: &std::path::Path) -> String {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("crawl");
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("pounce");
    let dir = path.parent();
    for n in 2..100 {
        let candidate = match dir {
            Some(dir) => dir.join(format!("{stem}-{n}.{ext}")),
            None => std::path::PathBuf::from(format!("{stem}-{n}.{ext}")),
        };
        if !candidate.exists() {
            return candidate.display().to_string();
        }
    }
    path.display().to_string()
}

/// Turns a seed that did not parse into one that might.
///
/// The overwhelmingly common mistake is a missing scheme — someone types
/// `example.com` because that is what a browser accepts. Only offered when the
/// corrected form actually parses, so the button never suggests something that
/// fails the same way.
pub fn seed_suggestion(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.contains("://") {
        return None;
    }
    let candidate = format!("https://{trimmed}");
    pounce_core::CrawlUrl::parse(&candidate)
        .ok()
        .map(|_| candidate)
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
    fn a_second_crawl_is_refused_while_one_is_running() {
        use pounce_core::CrawlStatus;
        assert!(may_start(None).is_ok(), "the first crawl may always start");
        assert!(may_start(Some(CrawlStatus::Running)).is_err());
        assert!(may_start(Some(CrawlStatus::Paused)).is_err());
        // Anything terminal frees the slot, including the ones that are not
        // success: a failed crawl must not lock the app out of trying again.
        for status in [
            CrawlStatus::Completed,
            CrawlStatus::Cancelled,
            CrawlStatus::Failed,
            CrawlStatus::CountLimitReached,
            CrawlStatus::TimeLimitReached,
        ] {
            assert!(
                may_start(Some(status)).is_ok(),
                "{status:?} should free the slot"
            );
        }
    }

    #[test]
    fn settings_become_limits_and_politeness() {
        let (limits, fetch) = to_engine(&CrawlSettings {
            seed: "http://e.com".into(),
            output: "/tmp/x.pounce".into(),
            images: false,
            max_depth: Some(3),
            max_urls: Some(1_000),
            max_duration_secs: Some(60),
            per_host_concurrency: Some(2),
            delay_ms: Some(250),
        })
        .unwrap();

        assert_eq!(limits.max_depth, Some(3));
        assert_eq!(limits.max_urls, Some(1_000));
        assert_eq!(
            limits.max_duration,
            Some(std::time::Duration::from_secs(60))
        );
        assert_eq!(fetch.max_concurrent_per_host, 2);
        assert_eq!(fetch.default_delay, std::time::Duration::from_millis(250));
    }

    #[test]
    fn omitted_settings_keep_the_engines_own_defaults() {
        let bare = CrawlSettings {
            seed: "http://e.com".into(),
            output: "/tmp/x.pounce".into(),
            images: false,
            max_depth: None,
            max_urls: None,
            max_duration_secs: None,
            per_host_concurrency: None,
            delay_ms: None,
        };
        let (limits, fetch) = to_engine(&bare).unwrap();
        let defaults = pounce_http::fetch::FetchConfig::default();
        assert_eq!(limits, pounce_core::CrawlLimits::default());
        assert_eq!(
            fetch.max_concurrent_per_host,
            defaults.max_concurrent_per_host
        );
        assert_eq!(fetch.default_delay, defaults.default_delay);
    }

    #[test]
    fn an_impolite_concurrency_is_refused_rather_than_clamped() {
        // Clamping would report a crawl that was not the one asked for.
        let mut settings = CrawlSettings {
            seed: "http://e.com".into(),
            output: "/tmp/x.pounce".into(),
            images: false,
            max_depth: None,
            max_urls: None,
            max_duration_secs: None,
            per_host_concurrency: Some(64),
            delay_ms: None,
        };
        assert!(matches!(to_engine(&settings), Err(ApiError::Crawl { .. })));
        settings.per_host_concurrency = Some(0);
        assert!(matches!(to_engine(&settings), Err(ApiError::Crawl { .. })));
        settings.per_host_concurrency = Some(MAX_PER_HOST_CONCURRENCY);
        assert!(
            to_engine(&settings).is_ok(),
            "the ceiling itself is allowed"
        );
    }

    #[test]
    fn a_limit_of_zero_is_refused_because_it_reads_as_unlimited() {
        let settings = CrawlSettings {
            seed: "http://e.com".into(),
            output: "/tmp/x.pounce".into(),
            images: false,
            max_depth: None,
            max_urls: Some(0),
            max_duration_secs: None,
            per_host_concurrency: None,
            delay_ms: None,
        };
        assert!(matches!(to_engine(&settings), Err(ApiError::Crawl { .. })));
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
