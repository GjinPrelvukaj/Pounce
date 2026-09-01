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

use rusqlite::{Connection, OpenFlags};
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
    #[error(
        "this file is at schema {found}, and a live crawl cannot be migrated to {known} while it is being written"
    )]
    NotMigrated { found: u32, known: u32 },
    #[error("this file is a database, but not a Pounce crawl")]
    NotACrawl,
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
    include_str!("migrations/012_page_detail.sql"),
    include_str!("migrations/013_has_issue.sql"),
    include_str!("migrations/014_meta_description_index.sql"),
    include_str!("migrations/015_sitemaps.sql"),
    include_str!("migrations/016_sitemap_truncated.sql"),
    include_str!("migrations/017_analysed.sql"),
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

    /// Opens a database for reading only, without migrating it.
    ///
    /// This exists for one job: reading a crawl **while the crawl is still
    /// writing it.** WAL gives one writer and many readers, so the grid can
    /// query rows that landed a moment ago — but only if the second connection
    /// never tries to write. `Store::open` would: it runs the migrations, and
    /// two connections racing `PRAGMA user_version` on one file is the
    /// two-writers hazard `may_start` exists to prevent, arrived at by a
    /// different door.
    ///
    /// Refuses a file that is not already at `SCHEMA_VERSION`, in both
    /// directions. Too new is unreadable; too old needs a migration this
    /// connection is not allowed to perform, and a live crawl's writer has
    /// already migrated the file before the first row lands — so "too old"
    /// here means the caller passed an archived file to the live path.
    pub fn open_read_only(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let store = Self { conn };
        let found = store.version()?;
        match found.cmp(&SCHEMA_VERSION) {
            std::cmp::Ordering::Greater => Err(StoreError::TooNew {
                found,
                known: SCHEMA_VERSION,
            }),
            std::cmp::Ordering::Less => Err(StoreError::NotMigrated {
                found,
                known: SCHEMA_VERSION,
            }),
            std::cmp::Ordering::Equal => Ok(store),
        }
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
        // Refuse to migrate a database that is not ours.
        //
        // `Store::open` creates the file when it is missing, so "empty file, no
        // tables" is the ordinary new-crawl case. A file that already has
        // tables and has never been migrated by us is somebody else's database
        // — someone dragged the wrong file onto the window — and running eleven
        // `CREATE TABLE`s into it would add our schema to their data. There is
        // no undo for that.
        if current == 0 && self.has_tables()? {
            return Err(StoreError::NotACrawl);
        }
        // A `user_version` we did not set. Other programs use the field, so a
        // foreign database can arrive claiming to be at schema 13 with none of
        // the tables that implies — and every later query would fail with "no
        // such table" from somewhere deep inside a join.
        if current > 0 && !self.has_pages()? {
            return Err(StoreError::NotACrawl);
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

    /// Whether the file holds any user tables at all. Views and indices count:
    /// anything in `sqlite_master` means somebody's data.
    fn has_tables(&self) -> Result<bool, StoreError> {
        let count: i64 = self.conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'",
            [],
            |r| r.get(0),
        )?;
        Ok(count > 0)
    }

    fn has_pages(&self) -> Result<bool, StoreError> {
        let count: i64 = self.conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'pages'",
            [],
            |r| r.get(0),
        )?;
        Ok(count > 0)
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
    /// Builds the inlink index alone, before the site rules run.
    ///
    /// Split out of `build_query_indices` because the two have different
    /// readers. Three site rules — `links.orphan-page`,
    /// `links.broken-internal` and `indexability.blocked-but-linked` — join on
    /// `links.target_url`, and they run *between* the crawl loop and the rest
    /// of the index build. Without this, SQLite has no choice but to re-scan
    /// the whole `links` table once per candidate row: measured at 10k pages,
    /// `links.orphan-page` alone took **45 s**, and the shape is
    /// O(pages x links), so a 500k crawl would never finish.
    ///
    /// This does not weaken migration 007's rule, it applies it. The rule is
    /// that an index nothing reads *during* a crawl is not maintained during
    /// one — 14M random TEXT inserts is what took throughput from 3,690 URL/s
    /// to 359. Building it once here is still the single sorted bulk build;
    /// it just happens a moment earlier, before its first reader instead of
    /// after.
    pub fn build_link_index(&self) -> Result<(), StoreError> {
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS links_target ON links (target_url)",
            [],
        )?;
        Ok(())
    }

    /// The remaining read-path indices, built once the crawl and its rules are
    /// done. `IF NOT EXISTS` makes the `links_target` line a no-op when
    /// `build_link_index` has already run.
    pub fn build_query_indices(&self) -> Result<(), StoreError> {
        self.conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS links_target ON links (target_url);
                 CREATE INDEX IF NOT EXISTS issues_page ON issues (page_id);
                 CREATE INDEX IF NOT EXISTS issues_url ON issues (url);
                 CREATE INDEX IF NOT EXISTS issues_rule ON issues (rule_id);
                 CREATE INDEX IF NOT EXISTS issues_severity ON issues (severity);
                 -- The overview's GROUP BY, served in index order rather than
                 -- through a temp B-tree: 472 ms -> 102 ms at 1M issues.
                 CREATE INDEX IF NOT EXISTS issues_rule_severity_url                      ON issues (rule_id, severity, url)",
        )?;
        // Repair `has_issue` before the composites read it. The writer keeps it
        // current for page rules, but *site* rules attach findings to pages
        // written long before — a duplicate title is only knowable once both
        // pages exist. One pass here is what makes the column true rather than
        // mostly true, and it is why the column is a cache and not a claim.
        self.conn.execute(
            "UPDATE pages SET has_issue = 1 WHERE has_issue = 0 \
             AND EXISTS (SELECT 1 FROM issues i WHERE i.page_id = pages.id)",
            [],
        )?;
        // The declared filter x sort composites. A single-column index serves
        // the filter or the order, never both, and the probe measured that gap
        // at 18 s on a 1M-row grid query.
        self.conn
            .execute_batch(&crate::query::composite_index_sql())?;
        Ok(())
    }

    /// Marks the end-of-crawl analysis as done.
    ///
    /// Called after the inlink index, the site rules, the sitemap pass and the
    /// read-path indices — everything that a stopped crawl skips. Until this
    /// is set, the file holds pages and no conclusions, and every view that
    /// depends on those steps has to say so rather than showing an empty
    /// result as though it were an answer.
    pub fn mark_analysed(&self) -> Result<(), StoreError> {
        self.conn
            .execute("UPDATE crawl SET analysed = 1 WHERE id = 1", [])?;
        Ok(())
    }

    /// Whether the end-of-crawl analysis ran. False for an interrupted crawl,
    /// and false for a crawl still in flight.
    ///
    /// The column is the fast answer; the *evidence* is the true one.
    /// Migration 017 backfills the flag, but only for files it has not already
    /// migrated — a file opened between that migration shipping and the
    /// backfill being added carries a permanent 0 and would be accused of
    /// being interrupted forever. `links_target` is built in the end-of-crawl
    /// block and nowhere else, so its presence settles it either way.
    pub fn is_analysed(&self) -> Result<bool, StoreError> {
        let flagged = self
            .conn
            .query_row("SELECT analysed FROM crawl WHERE id = 1", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap_or(0)
            != 0;
        if flagged {
            return Ok(true);
        }
        let built: i64 = self.conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'index' AND name = 'links_target'",
            [],
            |r| r.get(0),
        )?;
        Ok(built != 0)
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }
}
