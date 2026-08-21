//! The database file: pragmas, migrations, and the table layout.
//!
//! Migrations are numbered and applied by SQLite's own `user_version`, which
//! costs nothing and needs no table of its own. A `.pounce` file is a document
//! a user keeps, so opening an old one has to work; the version is the only
//! thing that makes that checkable rather than hopeful.
//!
//! The indices exist because of the load-bearing decision in the spec: the UI
//! never receives the dataset, it issues queries, and sorting is `ORDER BY` on
//! an indexed column. An unindexed sortable column would mean a full scan per
//! scroll, which is the failure mode this whole design exists to avoid — so
//! index cost at write time is a price already agreed to, not a regression.

use rusqlite::Connection;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error(
        "this file was written by a newer version of Pounce (schema {found}, this build knows {known})"
    )]
    TooNew { found: u32, known: u32 },
    #[error("invalid URL in crawl state `{url}`: {source}")]
    InvalidUrl {
        url: String,
        #[source]
        source: pounce_core::UrlError,
    },
    #[error("crawl limit `{0}` exceeds SQLite's integer range")]
    LimitTooLarge(&'static str),
    #[error("a redirect outcome must contain at least one hop")]
    EmptyRedirectChain,
}

/// Every migration, in order. The index in this array *is* the version, so an
/// applied migration is never edited — only appended to.
const MIGRATIONS: &[&str] = &[
    include_str!("migrations/001_pages.sql"),
    include_str!("migrations/002_links.sql"),
    include_str!("migrations/003_crawl_state.sql"),
    include_str!("migrations/004_crawl_failures.sql"),
    include_str!("migrations/005_crawl_limits.sql"),
    include_str!("migrations/006_crawl_redirects.sql"),
];

/// The schema version this build writes and can read.
pub const SCHEMA_VERSION: u32 = MIGRATIONS.len() as u32;

/// An open crawl database.
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Opens or creates the database at `path`, bringing it up to
    /// `SCHEMA_VERSION`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let conn = Connection::open(path)?;
        // WAL is what lets the UI read while the writer commits. Without it,
        // every batch blocks the reader and the app stutters mid-crawl.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        // NORMAL rather than FULL: under WAL this risks losing only the last
        // commits on an OS-level crash, never a corrupt file. A crawl is
        // re-runnable and resumable, so an fsync per batch buys durability
        // nobody needs at a cost measured in whole-crawl throughput.
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Self::prepare(conn)
    }

    /// An in-memory database, for tests. Not a crawl mode — storage is
    /// disk-backed from the first row, and nothing in the product may open one
    /// of these.
    pub fn in_memory() -> Result<Self, StoreError> {
        Self::prepare(Connection::open_in_memory()?)
    }

    fn prepare(conn: Connection) -> Result<Self, StoreError> {
        // Off by default in SQLite. A link row pointing at a deleted page is
        // exactly the corruption that yields a report with phantom inlinks.
        conn.pragma_update(None, "foreign_keys", true)?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    /// Applies every migration the file has not seen yet.
    ///
    /// `user_version` is SQLite's own counter, so this needs no bookkeeping
    /// table and cannot disagree with one.
    fn migrate(&self) -> Result<(), StoreError> {
        let current = self.version()?;
        if current > SCHEMA_VERSION {
            return Err(StoreError::TooNew {
                found: current,
                known: SCHEMA_VERSION,
            });
        }
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
            // One transaction per migration: a half-applied schema is worse
            // than an unopenable file, because it looks like it worked.
            self.conn.execute_batch(&format!(
                "BEGIN; {sql} PRAGMA user_version = {}; COMMIT;",
                i + 1
            ))?;
        }
        Ok(())
    }

    pub fn version(&self) -> Result<u32, StoreError> {
        Ok(self
            .conn
            .pragma_query_value(None, "user_version", |r| r.get(0))?)
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }
}
