//! Disk-backed storage for a crawl.
//!
//! Every row goes to SQLite as it is produced. There is no in-memory-then-dump
//! mode, and adding one would break two things at once: resumable crawls, and
//! the promise that memory stays flat regardless of crawl size. That promise is
//! the architecture — see `ARCHITECTURE.md` on query-don't-dump.

pub mod detail;
pub mod query;
pub mod schema;
pub mod state;
pub mod writer;

pub use detail::{DetailIssue, LinkRow, MAX_LINKS, PageDetail};
pub use query::{
    Comparison, Filter, FilterKind, FilterShape, FilterSpec, IssueCount, IssueOverview,
    MAX_COMPOSITE_INDICES, MAX_WINDOW, Page, QueryError, RowView, SortColumn, SortDirection,
    SortSpec,
};
pub use schema::{SCHEMA_VERSION, Store, StoreError};
pub use state::{CrawlState, FrontierEntry};
pub use writer::{BATCH_MAX_AGE, BATCH_SIZE, RedirectHop, Writer};
