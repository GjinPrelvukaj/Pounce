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
    include_str!("migrations/007_defer_links_target.sql"),
    include_str!("migrations/008_issues.sql"),
    include_str!("migrations/009_page_content.sql"),
    include_str!("migrations/010_issue_subject.sql"),
    include_str!("migrations/011_resources.sql"),
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
        // `cache_size` and `temp_store` are deliberately left at their
        // defaults. Both were tried on the theory that a 2 MB page cache
        // against a 4.7 GB database must be thrashing. Measured at 500k, both
        // were worse:
        //
        //   default cache   142.6 s   257 MB
        //   64 MB cache     152.8 s   356 MB   (slower *and* 99 MB heavier)
        //   temp_store=MEMORY         1,592 MB (failed the 400 MB gate)
        //
        // The write path is append-mostly once `links_target` is deferred, so
        // a bigger cache holds pages nothing reads again while competing with
        // the OS page cache that was already doing the job. Raising either one
        // needs a measurement showing it helps, not the intuition that it
        // should.
        // `temp_store` is deliberately left at its default (spill to file).
        //
        // Forcing sorts into memory looks free until it meets the deferred
        // index: building `links_target` over 14M rows is an external sort, and
        // MEMORY makes SQLite hold all of it. Measured at 500k, that took peak
        // RSS from 261 MB to **1,592 MB** and failed the 400 MB gate outright,
        // while saving ~10 s of a 142 s crawl. Flat memory is the product's
        // headline advantage; this is not a trade to make for it.
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

    /// Builds the indices that only a finished crawl needs.
    ///
    /// Separated from the migrations because maintaining them *during* a crawl
    /// is what makes throughput fall away with scale: a TEXT index over
    /// randomly-ordered URLs pays a cold B-tree page per insert, and there are
    /// one of those per discovered link. Building once at the end is a single
    /// sorted pass over data already on disk.
    ///
    /// Idempotent, and safe to call on a crawl that was interrupted — an
    /// unfinished file simply queries the link graph without the index until
    /// someone calls this.
    pub fn build_query_indices(&self) -> Result<(), StoreError> {
        self.conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS links_target ON links (target_url);
                 CREATE INDEX IF NOT EXISTS issues_page ON issues (page_id);
                 CREATE INDEX IF NOT EXISTS issues_url ON issues (url);
                 CREATE INDEX IF NOT EXISTS issues_rule ON issues (rule_id);
                 CREATE INDEX IF NOT EXISTS issues_severity ON issues (severity)",
        )?;
        Ok(())
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }
}
