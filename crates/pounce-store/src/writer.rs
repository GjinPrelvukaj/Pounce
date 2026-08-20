//! The batched writer.
//!
//! One transaction per ~500 records. The batch size is the whole point: SQLite
//! commits at a few hundred per second when each insert is its own transaction,
//! because each one is a durability barrier. Amortising that barrier across a
//! batch is the difference between the writer being the crawl's bottleneck and
//! being invisible — and a slow disk throttling the fetchers is exactly what
//! the bounded pipeline is supposed to arrange.
//!
//! There is no buffer of pending records. The transaction is left open and rows
//! go in as they arrive, so peak memory is one record rather than a batch of
//! them. The crash window is identical either way — up to one batch — and a
//! crawl is resumable, which is what makes that window acceptable.

use crate::schema::{Store, StoreError};
use pounce_parse::PageRecord;
use rusqlite::params;

/// Records per transaction.
///
/// Chosen from the spec rather than measured; the bench in `pounce-bench`
/// exists to say whether it is anywhere near right.
pub const BATCH_SIZE: usize = 500;

/// Every column the writer sets, in one place so the `INSERT` and its
/// conflict clause cannot drift apart.
const COLUMNS: &[&str] = &[
    "url",
    "status",
    "depth",
    "size",
    "truncated",
    "content_type",
    "charset",
    "kind",
    "content_type_mismatch",
    "elapsed_ms",
    "time_to_headers_ms",
    "redirect_chain",
    "title",
    "meta_description",
    "h1",
    "h2",
    "canonical",
    "canonical_url",
    "noindex",
    "nofollow",
    "noarchive",
    "nosnippet",
    "hreflang",
    "open_graph",
    "images",
    "word_count",
];

/// Upsert on `url`.
///
/// `INSERT OR REPLACE` would be shorter and wrong: it deletes the old row, so
/// the `id` changes and every link edge pointing at it is orphaned. A resumed
/// crawl re-fetching a URL must update the row in place.
fn insert_sql() -> String {
    let cols = COLUMNS.join(", ");
    let holes = (1..=COLUMNS.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let updates = COLUMNS
        .iter()
        .filter(|c| **c != "url")
        .map(|c| format!("{c} = excluded.{c}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("INSERT INTO pages ({cols}) VALUES ({holes}) ON CONFLICT(url) DO UPDATE SET {updates}")
}

pub struct Writer<'a> {
    store: &'a mut Store,
    batch_size: usize,
    /// Rows written inside the currently open transaction.
    in_batch: usize,
    /// Rows committed since this writer was created.
    committed: u64,
    open: bool,
}

impl<'a> Writer<'a> {
    pub fn new(store: &'a mut Store) -> Self {
        Self::with_batch_size(store, BATCH_SIZE)
    }

    pub fn with_batch_size(store: &'a mut Store, batch_size: usize) -> Self {
        Self {
            store,
            batch_size: batch_size.max(1),
            in_batch: 0,
            committed: 0,
            open: false,
        }
    }

    /// Writes one record, committing the batch if it is now full.
    pub fn push(&mut self, record: &PageRecord) -> Result<(), StoreError> {
        if !self.open {
            self.store.conn().execute_batch("BEGIN")?;
            self.open = true;
        }

        let robots = record.meta_robots;
        let kind = serde_json::to_value(record.kind)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| "undeclared".into());

        // prepare_cached so the SQL is parsed once for the whole crawl rather
        // than once per row; at 500 rows a batch that parse is the write.
        let mut stmt = self.store.conn().prepare_cached(&insert_sql())?;
        stmt.execute(params![
            record.url.to_string(),
            record.status,
            record.depth,
            record.size as i64,
            record.truncated,
            record.content_type,
            record.charset,
            kind,
            record.content_type_mismatch,
            record.elapsed_ms,
            record.time_to_headers_ms,
            to_json(&record.redirect_chain),
            record.title,
            record.meta_description,
            to_json(&record.h1),
            to_json(&record.h2),
            record.canonical,
            record.canonical_url.as_ref().map(|u| u.to_string()),
            robots.noindex,
            robots.nofollow,
            robots.noarchive,
            robots.nosnippet,
            to_json(&record.hreflang),
            to_json(&record.open_graph),
            to_json(&record.images),
            record.word_count,
        ])?;
        drop(stmt);

        self.in_batch += 1;
        if self.in_batch >= self.batch_size {
            self.flush()?;
        }
        Ok(())
    }

    /// Commits whatever is open. Returns the number of rows in that batch.
    pub fn flush(&mut self) -> Result<usize, StoreError> {
        if !self.open {
            return Ok(0);
        }
        self.store.conn().execute_batch("COMMIT")?;
        self.open = false;
        let n = std::mem::take(&mut self.in_batch);
        self.committed += n as u64;
        Ok(n)
    }

    /// Rows committed so far. Rows in an open batch are not counted, because
    /// they are not yet readable by anyone else.
    pub fn committed(&self) -> u64 {
        self.committed
    }
}

/// Serialises a repeating field.
///
/// Infallible in practice — every one of these is a `Vec` of plain data — so a
/// failure here means the record type grew something unserialisable, and an
/// empty array is a less damaging outcome than a panicking writer mid-crawl.
fn to_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "[]".into())
}

impl Drop for Writer<'_> {
    /// Rolls back rather than committing.
    ///
    /// A writer dropped without `flush` is a crawl that stopped unexpectedly,
    /// and quietly committing on the way out would make `committed()` a lie at
    /// the one moment it is read — during resume. The frontier decides what to
    /// re-fetch, so losing an uncommitted batch costs a few refetches and
    /// nothing else.
    fn drop(&mut self) {
        if self.open {
            let _ = self.store.conn().execute_batch("ROLLBACK");
        }
    }
}
