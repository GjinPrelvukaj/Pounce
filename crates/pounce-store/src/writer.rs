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
use crate::state::insert_frontier;
use pounce_core::CrawlUrl;
use pounce_parse::PageRecord;
use rusqlite::params;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RedirectHop {
    pub url: CrawlUrl,
    pub status: u16,
    pub location: String,
    pub target: Option<CrawlUrl>,
}

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
    "title_count",
    "body_hash",
];

/// Upsert on `url`.
///
/// The page id comes from a follow-up `SELECT` rather than a `RETURNING`
/// clause. `RETURNING` was tried and measured as a wash at 100k (medians 20.7 s
/// against 20.0 s, ranges overlapping), so it does not earn the larger diff —
/// the lookup is a unique index hit on a page SQLite has just touched.
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

    pub fn discover(&mut self, entries: &[(CrawlUrl, u16)]) -> Result<(), StoreError> {
        if entries.is_empty() {
            return Ok(());
        }
        self.begin()?;
        insert_frontier(self.store.conn(), entries)
    }

    /// Persists a terminal fetch outcome so resume does not retry it forever.
    pub fn fail(&mut self, url: &CrawlUrl, reason: &str) -> Result<(), StoreError> {
        self.begin()?;
        self.store.conn().execute(
            "INSERT INTO crawl_failures (url, reason) VALUES (?1, ?2) \
             ON CONFLICT(url) DO UPDATE SET reason = excluded.reason",
            params![url.to_string(), reason],
        )?;
        self.finish_row()
    }

    /// Persists a redirect chain as the terminal outcome for its source URL.
    pub fn redirect(
        &mut self,
        source: &CrawlUrl,
        final_url: Option<&CrawlUrl>,
        hops: &[RedirectHop],
        outcome: &str,
    ) -> Result<(), StoreError> {
        let first = hops.first().ok_or(StoreError::EmptyRedirectChain)?;
        self.begin()?;
        self.store.conn().execute(
            "INSERT INTO crawl_redirects (source_url, status, final_url, chain, outcome) \
             VALUES (?1, ?2, ?3, ?4, ?5) \
             ON CONFLICT(source_url) DO UPDATE SET \
             status = excluded.status, final_url = excluded.final_url, \
             chain = excluded.chain, outcome = excluded.outcome",
            params![
                source.to_string(),
                first.status,
                final_url.map(ToString::to_string),
                to_json(&hops),
                outcome,
            ],
        )?;
        self.finish_row()
    }

    /// Writes one record, committing the batch if it is now full.
    pub fn push(&mut self, record: &PageRecord) -> Result<i64, StoreError> {
        self.begin()?;

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
            record.title_count,
            // SQLite integers are signed; the bit pattern round-trips, and
            // bit-pattern equality is all a duplicate check asks.
            record.body_hash.map(|h| h as i64),
        ])?;
        drop(stmt);

        let page_id: i64 = self.store.conn().query_row(
            "SELECT id FROM pages WHERE url = ?1",
            [record.url.to_string()],
            |row| row.get(0),
        )?;

        self.store
            .conn()
            .execute("DELETE FROM links WHERE source_page_id = ?1", [page_id])?;
        let mut stmt = self.store.conn().prepare_cached(
            "INSERT INTO links \
             (source_page_id, href, target_url, anchor_text, nofollow) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for link in &record.links {
            stmt.execute(params![
                page_id,
                link.href,
                link.target.as_ref().map(ToString::to_string),
                link.text,
                link.nofollow,
            ])?;
        }
        drop(stmt);

        self.finish_row()?;
        Ok(page_id)
    }

    fn finish_row(&mut self) -> Result<(), StoreError> {
        self.in_batch += 1;
        if self.in_batch >= self.batch_size {
            self.flush()?;
        }
        Ok(())
    }

    /// Appends findings about `url` inside the open batch.
    ///
    /// `page_id` is `None` when the subject never became a page — a redirect
    /// loop, a hop-limit blowout, an unreachable host. Those are real findings
    /// and must be recordable, or `--fail-on` would pass a site full of them.
    ///
    /// Takes plain tuples rather than `pounce_audit::Issue` so that
    /// `pounce-store` does not depend on `pounce-audit`; the dependency runs
    /// the other way, and reversing it would make the two mutually dependent.
    pub fn issues(
        &mut self,
        url: &str,
        page_id: Option<i64>,
        issues: &[(&'static str, &'static str, Option<&str>)],
    ) -> Result<(), StoreError> {
        if issues.is_empty() {
            return Ok(());
        }
        self.begin()?;
        let mut stmt = self.store.conn().prepare_cached(
            "INSERT INTO issues (url, page_id, rule_id, severity, detail) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for (rule_id, severity, detail) in issues {
            stmt.execute(params![url, page_id, rule_id, severity, detail])?;
        }
        Ok(())
    }

    /// The stored id for a URL, or `None` if it was never crawled.
    ///
    /// Used only by site rules, which run once at the end over a bounded
    /// result set — never on the per-page hot path.
    pub fn page_id(&mut self, url: &str) -> Result<Option<i64>, StoreError> {
        self.begin()?;
        let mut stmt = self
            .store
            .conn()
            .prepare_cached("SELECT id FROM pages WHERE url = ?1")?;
        match stmt.query_row(params![url], |r| r.get(0)) {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn begin(&mut self) -> Result<(), StoreError> {
        if !self.open {
            self.store.conn().execute_batch("BEGIN")?;
            self.open = true;
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
