//! Disk-backed storage for a crawl.
//!
//! Every row goes to SQLite as it is produced. There is no in-memory-then-dump
//! mode, and adding one would break two things at once: resumable crawls, and
//! the promise that memory stays flat regardless of crawl size. That promise is
//! the architecture — see `ARCHITECTURE.md` on query-don't-dump.

pub mod schema;
pub mod writer;

pub use schema::{Store, StoreError};
pub use writer::{BATCH_SIZE, Writer};
