//! The read path the UI talks to.
//!
//! The load-bearing decision in the spec is that the UI never receives the
//! dataset: it sends a query and gets back the visible window. That makes this
//! module the boundary where a user's filter text becomes SQL, so **nothing
//! here interpolates a user's bytes into a statement**. Every filter compiles
//! to a fragment made of `&'static str` and `?` placeholders, and the values
//! travel separately as bound parameters.
//!
//! That is not a lint rule, it is the reason the module is shaped this way: a
//! `Filter` is a closed enum rather than a string, so there is no expressible
//! filter whose SQL a caller chose.

use crate::schema::{Store, StoreError};
use pounce_parse::BodyKind;
use rusqlite::types::Value;

/// How a numeric filter compares. Each maps to a fixed operator — the enum is
/// what keeps an operator from being something the user typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comparison {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl Comparison {
    fn op(self) -> &'static str {
        match self {
            Comparison::Eq => "=",
            Comparison::Ne => "<>",
            Comparison::Lt => "<",
            Comparison::Le => "<=",
            Comparison::Gt => ">",
            Comparison::Ge => ">=",
        }
    }
}

/// One condition on the grid.
///
/// `HasIssue` takes a `&'static str` because rule ids come from the registry:
/// a filter cannot name a rule that does not exist. `UrlContains` is the only
/// free-text case, and it binds a `LIKE` pattern rather than building one into
/// the SQL.
#[derive(Debug, Clone, PartialEq)]
pub enum Filter {
    Status(Comparison, u16),
    Depth(Comparison, u16),
    WordCount(Comparison, u32),
    Kind(BodyKind),
    Noindex(bool),
    /// Any issue at all, or one specific rule's.
    HasIssue(Option<&'static str>),
    UrlContains(String),
}

/// Which kind of filter, without its value. The unit a supported filter/sort
/// pair is declared in — see `SortSpec`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FilterKind {
    Status,
    Depth,
    WordCount,
    Kind,
    Noindex,
    HasIssue,
    UrlContains,
}

impl Filter {
    pub fn kind(&self) -> FilterKind {
        match self {
            Filter::Status(..) => FilterKind::Status,
            Filter::Depth(..) => FilterKind::Depth,
            Filter::WordCount(..) => FilterKind::WordCount,
            Filter::Kind(_) => FilterKind::Kind,
            Filter::Noindex(_) => FilterKind::Noindex,
            Filter::HasIssue(_) => FilterKind::HasIssue,
            Filter::UrlContains(_) => FilterKind::UrlContains,
        }
    }

    /// The SQL fragment and the values it binds.
    ///
    /// The fragment is built from `&'static str` and `?` only. Every value —
    /// including the `LIKE` pattern, wildcards and all — leaves through the
    /// parameter list.
    fn compile(&self) -> (String, Vec<Value>) {
        match self {
            Filter::Status(cmp, v) => (
                format!("p.status {} ?", cmp.op()),
                vec![Value::Integer(i64::from(*v))],
            ),
            Filter::Depth(cmp, v) => (
                format!("p.depth {} ?", cmp.op()),
                vec![Value::Integer(i64::from(*v))],
            ),
            Filter::WordCount(cmp, v) => (
                format!("p.word_count {} ?", cmp.op()),
                vec![Value::Integer(i64::from(*v))],
            ),
            Filter::Kind(kind) => (
                "p.kind = ?".into(),
                vec![Value::Text(kind_str(*kind).to_string())],
            ),
            Filter::Noindex(on) => ("p.noindex = ?".into(), vec![Value::Integer(i64::from(*on))]),
            Filter::HasIssue(None) => (
                "EXISTS (SELECT 1 FROM issues i WHERE i.page_id = p.id)".into(),
                vec![],
            ),
            Filter::HasIssue(Some(rule)) => (
                "EXISTS (SELECT 1 FROM issues i WHERE i.page_id = p.id AND i.rule_id = ?)".into(),
                vec![Value::Text((*rule).to_string())],
            ),
            // The pattern is a bound value, so `%` typed by the user is data,
            // not syntax. `escape` is not set: `_` and `%` inside the needle
            // widen the match rather than breaking out of it, which is the
            // behaviour a search box wants.
            Filter::UrlContains(needle) => (
                "p.url LIKE ?".into(),
                vec![Value::Text(format!("%{needle}%"))],
            ),
        }
    }
}

/// `BodyKind` as it is stored — the same string the writer puts in `kind`.
fn kind_str(kind: BodyKind) -> &'static str {
    match kind {
        BodyKind::Html => "html",
        BodyKind::Pdf => "pdf",
        BodyKind::Image => "image",
        BodyKind::Other => "other",
        BodyKind::Undeclared => "undeclared",
    }
}

/// The conditions on one query, ANDed.
///
/// Empty means unfiltered, which compiles to no `WHERE` at all rather than to
/// `WHERE 1=1` — the plan for an unfiltered grid should be the plan for a bare
/// scan, not one with a constant SQLite has to reason about.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FilterSpec {
    filters: Vec<Filter>,
}

impl FilterSpec {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, filter: Filter) -> Self {
        self.filters.push(filter);
        self
    }

    pub fn filters(&self) -> &[Filter] {
        &self.filters
    }

    pub fn is_empty(&self) -> bool {
        self.filters.is_empty()
    }

    /// The `WHERE` clause — including the leading keyword, or empty — and the
    /// values to bind, in order.
    pub fn compile(&self) -> (String, Vec<Value>) {
        if self.filters.is_empty() {
            return (String::new(), Vec::new());
        }
        let mut clauses = Vec::with_capacity(self.filters.len());
        let mut params = Vec::new();
        for filter in &self.filters {
            let (sql, mut values) = filter.compile();
            clauses.push(sql);
            params.append(&mut values);
        }
        (format!("WHERE {}", clauses.join(" AND ")), params)
    }
}

impl FromIterator<Filter> for FilterSpec {
    fn from_iter<T: IntoIterator<Item = Filter>>(iter: T) -> Self {
        Self {
            filters: iter.into_iter().collect(),
        }
    }
}

/// Errors a query can be rejected with before it reaches SQLite.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum QueryError {
    #[error("sorting by {sort} is not supported with a {filter} filter")]
    UnsupportedPair {
        filter: &'static str,
        sort: &'static str,
    },
}

// ---- sorting ---------------------------------------------------------------

/// A column the grid can sort by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SortColumn {
    Url,
    Status,
    Depth,
    Size,
    WordCount,
    ElapsedMs,
    Title,
}

impl SortColumn {
    /// The stored column. `&'static str`, so a sort cannot name a column the
    /// user chose either.
    pub fn column(self) -> &'static str {
        match self {
            SortColumn::Url => "url",
            SortColumn::Status => "status",
            SortColumn::Depth => "depth",
            SortColumn::Size => "size",
            SortColumn::WordCount => "word_count",
            SortColumn::ElapsedMs => "elapsed_ms",
            SortColumn::Title => "title",
        }
    }

    pub fn all() -> &'static [SortColumn] {
        &[
            SortColumn::Url,
            SortColumn::Status,
            SortColumn::Depth,
            SortColumn::Size,
            SortColumn::WordCount,
            SortColumn::ElapsedMs,
            SortColumn::Title,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortDirection {
    #[default]
    Asc,
    Desc,
}

impl SortDirection {
    fn keyword(self) -> &'static str {
        match self {
            SortDirection::Asc => "ASC",
            SortDirection::Desc => "DESC",
        }
    }
}

/// The filter kinds that need an index built with their sort column.
///
/// Chosen from measured selectivity, not taste — see
/// `docs/benchmarks/2026-08-24-filter-sort-pairs.md`. `status = 200`,
/// `kind = html` and `noindex = false` each match ~90% of a healthy crawl, and
/// `depth <= 2` matches 60%; an unselective filter is exactly the case where
/// SQLite walks the sort index and pays a table lookup per skipped row. A
/// selective filter needs nothing: it matches few enough rows that sorting
/// them is free.
///
/// `HasIssue` is absent deliberately. It compiles to an `EXISTS` against
/// `issues_page`, which measured 20–32 ms at 200k across every sort column —
/// the join is cheap enough that a composite would buy nothing.
///
/// A pair not listed here is still *supported* when it needs no index; see
/// `SUPPORTED_PAIRS`.
const COMPOSITE_PAIRS: &[(FilterKind, SortColumn)] = &[
    (FilterKind::Status, SortColumn::Url),
    (FilterKind::Status, SortColumn::Title),
    (FilterKind::Status, SortColumn::Size),
    (FilterKind::Status, SortColumn::WordCount),
    (FilterKind::Status, SortColumn::ElapsedMs),
    (FilterKind::Status, SortColumn::Depth),
    (FilterKind::Kind, SortColumn::Url),
    (FilterKind::Kind, SortColumn::Title),
    (FilterKind::Kind, SortColumn::Size),
    (FilterKind::Kind, SortColumn::WordCount),
    (FilterKind::Kind, SortColumn::ElapsedMs),
    (FilterKind::Kind, SortColumn::Status),
    (FilterKind::Noindex, SortColumn::Url),
    (FilterKind::Noindex, SortColumn::Title),
    (FilterKind::Noindex, SortColumn::Size),
    (FilterKind::Noindex, SortColumn::WordCount),
    (FilterKind::Noindex, SortColumn::ElapsedMs),
    (FilterKind::Noindex, SortColumn::Status),
    (FilterKind::Depth, SortColumn::WordCount),
    (FilterKind::Depth, SortColumn::ElapsedMs),
];

/// The ceiling on composite indices, so the twenty-first is a decision rather
/// than a commit. Each is cheap alone; twenty are hundreds of megabytes at 1M
/// rows, on a file the user keeps.
pub const MAX_COMPOSITE_INDICES: usize = 24;

/// The column a filter kind indexes on, or `None` when it has no single
/// column to index — `HasIssue` reads another table, `UrlContains` is a
/// substring match no B-tree can serve.
fn filter_column(kind: FilterKind) -> Option<&'static str> {
    match kind {
        FilterKind::Status => Some("status"),
        FilterKind::Depth => Some("depth"),
        FilterKind::WordCount => Some("word_count"),
        FilterKind::Kind => Some("kind"),
        FilterKind::Noindex => Some("noindex"),
        FilterKind::HasIssue | FilterKind::UrlContains => None,
    }
}

/// The index name for a composite pair. Deterministic, so the schema builder
/// and the test asserting the plan agree without sharing a list of strings.
pub fn composite_index_name(filter: FilterKind, sort: SortColumn) -> Option<String> {
    filter_column(filter).map(|col| format!("pages_{col}_{}", sort.column()))
}

/// Every `(filter, sort)` this build will run, with the reason each is safe.
///
/// The rule, in order:
///
/// - a filter sorting by **its own column** needs nothing — one index serves
///   the range and the order;
/// - a pair with a **composite index** is served by it;
/// - `HasIssue` with any sort is cheap, measured;
/// - `UrlContains` is supported **only** with a URL sort, where walking the
///   URL index tests the pattern from the index itself. With any other sort it
///   costs a table lookup per skipped row and no index can fix that;
/// - `WordCount` and `Depth` filters with the cheap sorts measured under the
///   threshold are supported unindexed.
pub fn is_supported(filter: FilterKind, sort: SortColumn) -> bool {
    if filter_column(filter) == Some(sort.column()) {
        return true;
    }
    if COMPOSITE_PAIRS.contains(&(filter, sort)) {
        return true;
    }
    match filter {
        FilterKind::HasIssue => true,
        FilterKind::UrlContains => sort == SortColumn::Url,
        // Measured at 200k with single-column indices only: every one of these
        // came in under 35 ms, against pairs that cost 100–240 ms.
        FilterKind::Depth => matches!(
            sort,
            SortColumn::Url | SortColumn::Status | SortColumn::Size | SortColumn::Title
        ),
        FilterKind::WordCount => matches!(
            sort,
            SortColumn::Url | SortColumn::Status | SortColumn::Size | SortColumn::Title
        ),
        _ => false,
    }
}

/// A sort the store will run, with the filters it was checked against.
///
/// Built through `new`, which is the only way to make one — an unsupported
/// pair is refused here rather than discovered as an 18-second query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortSpec {
    column: SortColumn,
    direction: SortDirection,
}

impl SortSpec {
    /// Checks the sort against **every** filter in the spec. One unsupported
    /// filter is enough to make the query slow, regardless of the others.
    pub fn new(
        filters: &FilterSpec,
        column: SortColumn,
        direction: SortDirection,
    ) -> Result<Self, QueryError> {
        for filter in filters.filters() {
            let kind = filter.kind();
            if !is_supported(kind, column) {
                return Err(QueryError::UnsupportedPair {
                    filter: kind.name(),
                    sort: column.column(),
                });
            }
        }
        Ok(Self { column, direction })
    }

    pub fn column(&self) -> SortColumn {
        self.column
    }

    pub fn direction(&self) -> SortDirection {
        self.direction
    }

    /// `ORDER BY`, including the leading keyword.
    ///
    /// The id tie-break is not decoration: `OFFSET` over a non-unique sort key
    /// is only stable if the order is total, and an unstable order means a row
    /// can appear in two consecutive windows or in neither while scrolling.
    pub fn compile(&self) -> String {
        format!(
            "ORDER BY p.{} {}, p.id {}",
            self.column.column(),
            self.direction.keyword(),
            self.direction.keyword()
        )
    }
}

impl FilterKind {
    pub fn name(self) -> &'static str {
        match self {
            FilterKind::Status => "status",
            FilterKind::Depth => "depth",
            FilterKind::WordCount => "word_count",
            FilterKind::Kind => "kind",
            FilterKind::Noindex => "noindex",
            FilterKind::HasIssue => "has_issue",
            FilterKind::UrlContains => "url_contains",
        }
    }
}

/// The composite indices a finished crawl needs, as `CREATE INDEX` statements.
///
/// Built by `Store::build_query_indices` rather than kept during the crawl:
/// the same rule migration 007 applies to `links_target` — an index nothing
/// reads during a crawl is not maintained during one.
pub(crate) fn composite_index_sql() -> String {
    COMPOSITE_PAIRS
        .iter()
        .filter_map(|(filter, sort)| {
            let col = filter_column(*filter)?;
            let name = composite_index_name(*filter, *sort)?;
            Some(format!(
                "CREATE INDEX IF NOT EXISTS {name} ON pages ({col}, {});",
                sort.column()
            ))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every declared composite pair, for the tests that assert their plans.
pub fn composite_pairs() -> &'static [(FilterKind, SortColumn)] {
    COMPOSITE_PAIRS
}

// ---- the window ------------------------------------------------------------

/// One grid row: the nine columns the table shows, and nothing else.
///
/// Not a `PageRecord`. The detail pane fetches the rest one row at a time, and
/// widening this type is how "the UI never receives the dataset" gets lost a
/// column at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowView {
    pub id: i64,
    pub url: String,
    pub status: u16,
    pub depth: u16,
    pub size: i64,
    pub word_count: u32,
    pub title: Option<String>,
    pub kind: String,
    pub noindex: bool,
}

/// A window, and how many rows it was taken from.
///
/// `total` is what the scrollbar is sized by — the invariant is that scroll
/// position maps to `OFFSET`, and a scrollbar cannot be drawn without knowing
/// how far it goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub rows: Vec<RowView>,
    pub total: u64,
    pub offset: u64,
    /// The limit actually used, after clamping.
    pub limit: u32,
}

/// The most rows one query may return.
///
/// A caller asking for 100,000 gets this instead. The invariant is that the UI
/// never receives the dataset, and an unclamped limit leaves that invariant
/// resting on the caller's manners — including a caller that is a future
/// version of our own UI.
pub const MAX_WINDOW: u32 = 1_000;

const ROW_COLUMNS: &str =
    "p.id, p.url, p.status, p.depth, p.size, p.word_count, p.title, p.kind, p.noindex";

fn row_from(row: &rusqlite::Row<'_>) -> rusqlite::Result<RowView> {
    Ok(RowView {
        id: row.get(0)?,
        url: row.get(1)?,
        status: row.get(2)?,
        depth: row.get(3)?,
        size: row.get(4)?,
        word_count: row.get(5)?,
        title: row.get(6)?,
        kind: row.get(7)?,
        noindex: row.get::<_, i64>(8)? != 0,
    })
}

impl Store {
    /// One window of the grid, plus the size of the result it came from.
    ///
    /// `limit` is clamped to `MAX_WINDOW`. `offset` past the end returns no
    /// rows and the true total, which is what lets the UI recover from a
    /// scroll position that a re-filter invalidated.
    pub fn query_rows(
        &self,
        filters: &FilterSpec,
        sort: &SortSpec,
        offset: u64,
        limit: u32,
    ) -> Result<Page, StoreError> {
        let limit = limit.min(MAX_WINDOW);
        let (where_sql, params) = filters.compile();

        // Two statements rather than a window function: `count(*)` over the
        // filter alone uses the filter's own index, while the row query uses
        // the composite. Asking for both in one statement gives one plan for
        // two different jobs.
        let total: i64 = self
            .conn()
            .prepare_cached(&format!("SELECT count(*) FROM pages p {where_sql}"))?
            .query_row(rusqlite::params_from_iter(params.iter()), |r| r.get(0))?;

        let sql = format!(
            "SELECT {ROW_COLUMNS} FROM pages p {where_sql} {} LIMIT ?{} OFFSET ?{}",
            sort.compile(),
            params.len() + 1,
            params.len() + 2
        );
        let mut stmt = self.conn().prepare_cached(&sql)?;
        let mut bound = params.clone();
        bound.push(Value::Integer(i64::from(limit)));
        bound.push(Value::Integer(offset as i64));
        let rows = stmt
            .query_map(rusqlite::params_from_iter(bound.iter()), row_from)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Page {
            rows,
            total: total as u64,
            offset,
            limit,
        })
    }
}

// ---- the issue overview ----------------------------------------------------

/// One row of the overview: a rule, and how much of the crawl it fired on.
///
/// `urls` is not `issues`. A rule can fire twice on one page — two oversized
/// images, say — and an overview that reported only the issue count would make
/// one bad template look like a site-wide problem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueCount {
    pub rule_id: String,
    pub severity: String,
    pub issues: u64,
    pub urls: u64,
}

/// The overview screen's whole dataset. Small by construction — one row per
/// rule that fired, capped by the rule registry — so this one *is* returned
/// whole.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IssueOverview {
    /// Worst first: most issues, then rule id so the order is total.
    pub by_rule: Vec<IssueCount>,
    /// Severity, and the issues carrying it.
    pub by_severity: Vec<(String, u64)>,
    pub total_issues: u64,
    /// URLs with at least one issue of any rule.
    pub urls_with_issues: u64,
}

impl Store {
    /// Counts for the issue overview.
    ///
    /// Reads `issues` alone. Joining `pages` would cost a lookup per issue to
    /// fetch columns nothing here shows, and would silently drop the findings
    /// whose subject never became a page — a redirect loop, an unreachable
    /// host. Those are the findings a report most needs to carry.
    pub fn issue_overview(&self) -> Result<IssueOverview, StoreError> {
        let mut by_rule = Vec::new();
        {
            let mut stmt = self.conn().prepare_cached(
                "SELECT rule_id, severity, count(*), count(DISTINCT url) FROM issues \
                 GROUP BY rule_id, severity ORDER BY count(*) DESC, rule_id ASC",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(IssueCount {
                    rule_id: r.get(0)?,
                    severity: r.get(1)?,
                    issues: r.get::<_, i64>(2)? as u64,
                    urls: r.get::<_, i64>(3)? as u64,
                })
            })?;
            for row in rows {
                by_rule.push(row?);
            }
        }

        let mut by_severity = Vec::new();
        {
            let mut stmt = self.conn().prepare_cached(
                "SELECT severity, count(*) FROM issues GROUP BY severity ORDER BY count(*) DESC, \
                 severity ASC",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as u64)))?;
            for row in rows {
                by_severity.push(row?);
            }
        }

        let (total_issues, urls_with_issues): (i64, i64) = self.conn().query_row(
            "SELECT count(*), count(DISTINCT url) FROM issues",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;

        Ok(IssueOverview {
            by_rule,
            by_severity,
            total_issues: total_issues as u64,
            urls_with_issues: urls_with_issues as u64,
        })
    }
}
