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

/// What a filter's constraint looks like to a B-tree.
///
/// This is the distinction the 1M gate run turned up, and it is not visible in
/// the column: `status = 200` and `status >= 400` are the same `FilterKind` and
/// get completely different plans. Only an **equality** on the leading column
/// lets a composite `(filter, sort)` index return rows already in sort order.
/// A range leaves SQLite walking the sort index and fetching the row to test
/// the filter — measured at 1M, that is 34–101 ms for most sort columns and
/// ~450 ms for `word_count` and `elapsed_ms`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterShape {
    /// `= value` on an indexed column. A composite serves it.
    Equality,
    /// `<`, `<=`, `>`, `>=`, `<>` — no composite can serve it with another sort.
    Range,
    /// `EXISTS` against `issues`; a per-row lookup on `issues_page`.
    Exists,
    /// `LIKE %needle%`; no B-tree serves a substring.
    Substring,
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

    /// How this filter constrains an index — see `FilterShape`.
    pub fn shape(&self) -> FilterShape {
        match self {
            Filter::Status(Comparison::Eq, _)
            | Filter::Depth(Comparison::Eq, _)
            | Filter::WordCount(Comparison::Eq, _) => FilterShape::Equality,
            Filter::Status(..) | Filter::Depth(..) | Filter::WordCount(..) => FilterShape::Range,
            Filter::Kind(_) | Filter::Noindex(_) => FilterShape::Equality,
            // "any issue" reads the flag on `pages`; "this rule's issues" has no
            // column to read and stays a per-row lookup into `issues`.
            Filter::HasIssue(None) => FilterShape::Equality,
            Filter::HasIssue(Some(_)) => FilterShape::Exists,
            Filter::UrlContains(_) => FilterShape::Substring,
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
            // The denormalised flag, not `EXISTS`: an equality the composites
            // can serve. `EXISTS` cost one subquery per row the OFFSET skipped
            // — 180-220 ms at 1M, growing with scroll depth. See migration 013.
            Filter::HasIssue(None) => ("p.has_issue = 1".into(), vec![]),
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
                vec![Value::Text(format!("%{}%", percent_encode_needle(needle)))],
            ),
        }
    }
}

/// Percent-encodes a search needle the way a URL's path already is.
///
/// URLs are stored canonical, so `/café/` is on disk as `/caf%C3%A9/`. A user
/// typing `café` into the URL box would otherwise match nothing at all on a
/// site that has any non-ASCII path — the failure being invisible, because an
/// empty grid looks like an answer.
///
/// ASCII passes through untouched, so searching for `%C3%A9` still works too.
/// This does **not** handle an internationalised *host*: `münchen.de` is stored
/// punycoded as `xn--mnchen-3ya.de`, and no character-level rewrite reaches
/// that. Searching a host by its Unicode spelling is a known gap.
fn percent_encode_needle(needle: &str) -> String {
    let mut out = String::with_capacity(needle.len());
    for ch in needle.chars() {
        if ch.is_ascii() {
            out.push(ch);
        } else {
            let mut buf = [0u8; 4];
            for byte in ch.encode_utf8(&mut buf).as_bytes() {
                out.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    out
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

/// The `(filter, sort)` pairs that get a composite index.
///
/// Chosen from measured selectivity and then **corrected by the 1M gate run**,
/// which found the thing 200k hid: a composite only helps when the filter is an
/// *equality* on its leading column. `depth = 2` sorted by size costs 282 ms
/// without one and 1.6 ms with; `depth <= 2` sorted by size is 101 ms either
/// way, because SQLite walks the sort index and tests the filter per row
/// regardless.
///
/// So the set covers **every sort column** for the four filter kinds whose
/// equality case is both common and unselective — `status`, `kind`, `noindex`,
/// `depth`. The first three match ~90% of a healthy crawl; `depth` is the level
/// filter. `word_count` gets none: filtering for an exact word count is not a
/// thing anyone does, and its range case cannot use one anyway.
///
/// `HasIssue` is absent deliberately. It compiles to `EXISTS` against
/// `issues_page`, measured 56–86 ms at 1M across every sort column but one.
/// See `is_supported` for the exception.
///
/// See `docs/benchmarks/2026-08-24-filter-sort-pairs.md` and
/// `docs/benchmarks/2026-08-24-gate-m3.md`.
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
    (FilterKind::Kind, SortColumn::Depth),
    (FilterKind::Noindex, SortColumn::Url),
    (FilterKind::Noindex, SortColumn::Title),
    (FilterKind::Noindex, SortColumn::Size),
    (FilterKind::Noindex, SortColumn::WordCount),
    (FilterKind::Noindex, SortColumn::ElapsedMs),
    (FilterKind::Noindex, SortColumn::Status),
    (FilterKind::Noindex, SortColumn::Depth),
    (FilterKind::Depth, SortColumn::Url),
    (FilterKind::Depth, SortColumn::Title),
    (FilterKind::Depth, SortColumn::Size),
    (FilterKind::Depth, SortColumn::WordCount),
    (FilterKind::Depth, SortColumn::ElapsedMs),
    (FilterKind::Depth, SortColumn::Status),
    (FilterKind::HasIssue, SortColumn::Url),
    (FilterKind::HasIssue, SortColumn::Title),
    (FilterKind::HasIssue, SortColumn::Size),
    (FilterKind::HasIssue, SortColumn::WordCount),
    (FilterKind::HasIssue, SortColumn::ElapsedMs),
    (FilterKind::HasIssue, SortColumn::Status),
    (FilterKind::HasIssue, SortColumn::Depth),
];

/// The ceiling on composite indices, so the twenty-first is a decision rather
/// than a commit. Each is cheap alone; twenty are hundreds of megabytes at 1M
/// rows, on a file the user keeps.
pub const MAX_COMPOSITE_INDICES: usize = 36;

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
        // Only the unnamed case reads this column; `HasIssue(Some(rule))` is an
        // `EXISTS`, and `is_supported` splits them by shape.
        FilterKind::HasIssue => Some("has_issue"),
        FilterKind::UrlContains => None,
    }
}

/// The index name for a composite pair. Deterministic, so the schema builder
/// and the test asserting the plan agree without sharing a list of strings.
pub fn composite_index_name(filter: FilterKind, sort: SortColumn) -> Option<String> {
    filter_column(filter).map(|col| format!("pages_{col}_{}", sort.column()))
}

/// Sort columns a **range** filter may be combined with.
///
/// A range walks the sort index and fetches each row to test the filter, so the
/// cost is the sort index's, not the filter's. Measured at 1M with a window
/// half way into the match set: url 72 ms, status 34 ms, depth 26 ms, size
/// 101 ms, title 42 ms — and `word_count` 480 ms, `elapsed_ms` 427 ms. The two
/// slow ones are the low-cardinality columns, where every skipped row is a
/// table lookup landing somewhere else in the file.
const RANGE_SAFE_SORTS: &[SortColumn] = &[
    SortColumn::Url,
    SortColumn::Status,
    SortColumn::Depth,
    SortColumn::Size,
    SortColumn::Title,
];

/// Every `(filter, sort)` this build will run, with the reason each is safe.
///
/// The rule, in order:
///
/// - a filter sorting by **its own column** needs nothing — one index serves
///   the range and the order;
/// - an **equality** filter with a composite index is served by it, 1.4–2 ms
///   at 1M;
/// - `HasIssue` with any sort but `word_count`, which measured 275 ms against a
///   300 ms gate — passing, but not by enough to promise. The fix if it is ever
///   wanted is a denormalised flag on `pages`, which turns it into an equality
///   filter; that is a copy to keep in step, so it waits until someone needs
///   it;
/// - `UrlContains` **only** with a URL sort, where walking the URL index tests
///   the pattern from the index itself. With any other sort it is a table
///   lookup per skipped row, and no B-tree serves a substring match;
/// - anything else — including an equality filter with no composite — is
///   allowed only with the sort columns a range was measured safe with.
pub fn is_supported(filter: &Filter, sort: SortColumn) -> bool {
    let kind = filter.kind();
    if filter_column(kind) == Some(sort.column()) {
        return true;
    }
    match filter.shape() {
        FilterShape::Equality if COMPOSITE_PAIRS.contains(&(kind, sort)) => true,
        FilterShape::Exists => sort != SortColumn::WordCount,
        FilterShape::Substring => sort == SortColumn::Url,
        _ => RANGE_SAFE_SORTS.contains(&sort),
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
            if !is_supported(filter, column) {
                return Err(QueryError::UnsupportedPair {
                    filter: filter.kind().name(),
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
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
    /// Absent is not empty, all the way out to the grid: a page with no
    /// description and one with `content=""` are different findings.
    pub meta_description: Option<String>,
    pub canonical: Option<String>,
    pub elapsed_ms: i64,
}

/// A window, and how many rows it was taken from.
///
/// `total` is what the scrollbar is sized by — the invariant is that scroll
/// position maps to `OFFSET`, and a scrollbar cannot be drawn without knowing
/// how far it goes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
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

/// The projection every window fetch carries.
///
/// Twelve scalar columns from `pages` and nothing else — no join, no JSON.
/// `meta_description`, `canonical` and `elapsed_ms` are here because the grid's
/// views need them as columns and the row shape is where a grid column lives;
/// the repeating fields stay in `page_detail` where only the detail pane reads
/// them. At 200 rows a window this adds a few tens of kilobytes, which is a
/// window, not a dataset.
const ROW_COLUMNS: &str = "p.id, p.url, p.status, p.depth, p.size, p.word_count, \
     p.title, p.kind, p.noindex, p.meta_description, p.canonical, p.elapsed_ms";

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
        meta_description: row.get(9)?,
        canonical: row.get(10)?,
        elapsed_ms: row.get(11)?,
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
        // Clamped, not cast: `offset as i64` wraps negative past `i64::MAX`, and
        // SQLite reads a negative OFFSET as zero — so a grid whose scroll
        // position was computed from a stale total would silently jump to the
        // top of the list instead of showing an empty tail.
        bound.push(Value::Integer(offset.min(i64::MAX as u64) as i64));
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueCount {
    pub rule_id: String,
    pub severity: String,
    pub issues: u64,
    pub urls: u64,
}

/// The overview screen's whole dataset. Small by construction — one row per
/// rule that fired, capped by the rule registry — so this one *is* returned
/// whole.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
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

        // `count(DISTINCT url)` over every issue costs a temp B-tree on a TEXT
        // column — 88 ms at 1M. Distinct *page ids* is an ordered walk of
        // `issues_page` instead, 10 ms, and the findings with no page are
        // counted separately: `count(DISTINCT)` ignores NULLs, so without the
        // second query a crawl's redirect loops and unreachable hosts would be
        // missing from its own headline number. A URL cannot appear in both,
        // because the same URL always resolves to the same page id.
        let (total_issues, pages_with_issues): (i64, i64) = self.conn().query_row(
            "SELECT count(*), count(DISTINCT page_id) FROM issues",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let pageless: i64 = self.conn().query_row(
            "SELECT count(DISTINCT url) FROM issues WHERE page_id IS NULL",
            [],
            |r| r.get(0),
        )?;

        Ok(IssueOverview {
            by_rule,
            by_severity,
            total_issues: total_issues as u64,
            urls_with_issues: (pages_with_issues + pageless) as u64,
        })
    }
}

/// What a crawl is made of, for the overview panel.
///
/// Deliberately *not* about findings — `issue_overview` answers "what is wrong"
/// and this answers "what is here". A crawl of ten thousand pages where nine
/// thousand are images is a different site from one where nine thousand are
/// HTML, and no list of rule counts tells you which you are looking at.
///
/// Every count is one index-only query. `pages` carries an index on `kind`,
/// `status` and `noindex`, so none of these touches a table row — which is what
/// keeps the panel affordable to redraw once a second while a crawl writes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrawlOverview {
    /// Pages with a row: everything fetched and stored.
    pub crawled: u64,
    /// URLs discovered and not yet fetched. Zero on a finished crawl.
    pub queued: u64,
    /// URLs that never produced a response — DNS failures, timeouts, and
    /// anything robots.txt disallowed.
    pub failed: u64,
    /// `[html, pdf, image, other, undeclared]`, in that order.
    pub by_kind: Vec<(String, u64)>,
    /// `[1xx, 2xx, 3xx, 4xx, 5xx]`.
    pub by_class: [u64; 5],
    pub indexable: u64,
    pub noindex: u64,
}

impl Store {
    pub fn crawl_overview(&self) -> Result<CrawlOverview, StoreError> {
        let scalar = |sql: &str| -> Result<u64, StoreError> {
            Ok(self.conn().query_row(sql, [], |r| r.get::<_, i64>(0))? as u64)
        };

        let mut by_kind = Vec::new();
        {
            let mut stmt = self
                .conn()
                .prepare_cached("SELECT kind, count(*) FROM pages GROUP BY kind")?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
            for row in rows {
                let (kind, count) = row?;
                by_kind.push((kind, count as u64));
            }
        }
        // Ordered so the panel reads the same on every crawl, with the kinds a
        // site actually has first and the rest absent rather than zeroed.
        let order = ["html", "pdf", "image", "other", "undeclared"];
        by_kind
            .sort_by_key(|(kind, _)| order.iter().position(|k| k == kind).unwrap_or(order.len()));

        let mut by_class = [0u64; 5];
        for (i, class) in by_class.iter_mut().enumerate() {
            let low = (i as u16 + 1) * 100;
            *class = scalar(&format!(
                "SELECT count(*) FROM pages WHERE status >= {low} AND status < {}",
                low + 100
            ))?;
        }

        Ok(CrawlOverview {
            crawled: scalar("SELECT count(*) FROM pages")?,
            queued: scalar(
                "SELECT count(*) FROM frontier f LEFT JOIN pages p ON p.url = f.url \
                 WHERE p.id IS NULL",
            )?,
            failed: scalar("SELECT count(*) FROM crawl_failures")?,
            by_kind,
            by_class,
            indexable: scalar("SELECT count(*) FROM pages WHERE noindex = 0")?,
            noindex: scalar("SELECT count(*) FROM pages WHERE noindex = 1")?,
        })
    }
}
