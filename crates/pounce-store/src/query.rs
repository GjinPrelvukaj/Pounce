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
