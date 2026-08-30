//! What the site says it has, beside what the crawl found.
//!
//! Two lists that are supposed to agree and rarely do. The comparison is the
//! product: a sitemap URL no link reaches is an orphan its owner believes is
//! fine, and a crawled page absent from the sitemap is one they do not know
//! they have. Neither shows up in either list on its own.

use crate::schema::{Store, StoreError};
use rusqlite::OptionalExtension;

/// One sitemap document the crawl fetched.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SitemapFile {
    pub url: String,
    pub status: u16,
    pub urls: u64,
    pub is_index: bool,
    /// `robots`, `guess`, or the index that listed it.
    pub found_by: String,
    /// True when the document listed more URLs than the reader stores.
    pub truncated: bool,
}

/// One URL a sitemap listed, and what the crawl made of it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SitemapUrl {
    pub url: String,
    pub source: String,
    /// The response the crawl got, or `None` if it never reached this URL —
    /// which is the finding: a page the site advertises and no link reaches.
    pub status: Option<u16>,
    pub page_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SitemapPage {
    pub rows: Vec<SitemapUrl>,
    pub total: u64,
    pub offset: u64,
    pub limit: u32,
}

/// The two disagreements, counted.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SitemapSummary {
    /// Sitemap documents fetched, including the ones that failed.
    pub files: u64,
    pub urls: u64,
    /// Listed in a sitemap and never reached by the crawl.
    pub not_crawled: u64,
    /// Crawled, indexable, and in no sitemap.
    ///
    /// `None` when a sitemap was **truncated**: past the cap we do not know
    /// what the site listed, so every unmatched page might be listed after all.
    /// An `Option` rather than a number with a caveat, because a caveat is
    /// something a caller can forget to read and a `None` is not.
    pub not_listed: Option<u64>,
    /// robots.txt as served, when there was one.
    pub robots: Option<String>,
    pub robots_status: Option<u16>,
}

impl Store {
    /// Records one fetched sitemap document.
    pub fn put_sitemap(&self, file: &SitemapFile) -> Result<(), StoreError> {
        self.conn().execute(
            "INSERT INTO sitemaps (url, status, urls, is_index, found_by, truncated) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT(url) DO UPDATE SET \
             status = excluded.status, urls = excluded.urls, \
             is_index = excluded.is_index, found_by = excluded.found_by, \
             truncated = excluded.truncated",
            rusqlite::params![
                file.url,
                file.status,
                file.urls as i64,
                file.is_index as i64,
                file.found_by,
                file.truncated as i64
            ],
        )?;
        Ok(())
    }

    /// Records the URLs one sitemap listed. Ignores a URL already recorded:
    /// the first sitemap that named it is the one a finding should point at.
    pub fn put_sitemap_urls(&mut self, source: &str, urls: &[String]) -> Result<(), StoreError> {
        let tx = self.conn_mut().transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO sitemap_urls (url, source) VALUES (?1, ?2) \
                 ON CONFLICT(url) DO NOTHING",
            )?;
            for url in urls {
                stmt.execute(rusqlite::params![url, source])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn put_robots(
        &self,
        origin: &str,
        status: u16,
        body: Option<&str>,
    ) -> Result<(), StoreError> {
        self.conn().execute(
            "INSERT INTO robots_files (origin, status, body) VALUES (?1, ?2, ?3) \
             ON CONFLICT(origin) DO UPDATE SET status = excluded.status, body = excluded.body",
            rusqlite::params![origin, status, body],
        )?;
        Ok(())
    }

    pub fn sitemap_files(&self) -> Result<Vec<SitemapFile>, StoreError> {
        let mut stmt = self.conn().prepare_cached(
            "SELECT url, status, urls, is_index, found_by, truncated FROM sitemaps \
             ORDER BY url",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(SitemapFile {
                    url: r.get(0)?,
                    status: r.get(1)?,
                    urls: r.get::<_, i64>(2)? as u64,
                    is_index: r.get::<_, i64>(3)? != 0,
                    found_by: r.get(4)?,
                    truncated: r.get::<_, i64>(5)? != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// One window of the sitemap's URLs, each with what the crawl found there.
    ///
    /// `LEFT JOIN pages` is the right join here and the wrong one in the grid:
    /// this table is at most 50,000 rows per document rather than a million,
    /// and the join is the *question* rather than an extra column beside it.
    pub fn sitemap_urls(&self, offset: u64, limit: u32) -> Result<SitemapPage, StoreError> {
        let limit = limit.min(crate::query::MAX_WINDOW);
        let total: i64 = self
            .conn()
            .prepare_cached("SELECT count(*) FROM sitemap_urls")?
            .query_row([], |r| r.get(0))?;

        let mut stmt = self.conn().prepare_cached(
            "SELECT s.url, s.source, p.status, p.id FROM sitemap_urls s \
             LEFT JOIN pages p ON p.url = s.url ORDER BY s.url LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt
            .query_map(
                rusqlite::params![i64::from(limit), offset.min(i64::MAX as u64) as i64],
                |r| {
                    Ok(SitemapUrl {
                        url: r.get(0)?,
                        source: r.get(1)?,
                        status: r.get(2)?,
                        page_id: r.get(3)?,
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(SitemapPage {
            rows,
            total: total as u64,
            offset,
            limit,
        })
    }

    pub fn sitemap_summary(&self) -> Result<SitemapSummary, StoreError> {
        let files: i64 = self
            .conn()
            .query_row("SELECT count(*) FROM sitemaps", [], |r| r.get(0))?;
        let urls: i64 = self
            .conn()
            .query_row("SELECT count(*) FROM sitemap_urls", [], |r| r.get(0))?;
        let not_crawled: i64 = self.conn().query_row(
            "SELECT count(*) FROM sitemap_urls s \
             WHERE NOT EXISTS (SELECT 1 FROM pages p WHERE p.url = s.url)",
            [],
            |r| r.get(0),
        )?;
        // Indexable pages only. A page carrying `noindex` is deliberately kept
        // out of search, so its absence from the sitemap is agreement rather
        // than a finding — reporting it would bury the real ones.
        // Past a truncated sitemap we do not know what the site listed, so
        // this comparison cannot be made — and reporting it anyway would count
        // every URL beyond the cap as a page the site forgot to list.
        let truncated: i64 = self.conn().query_row(
            "SELECT count(*) FROM sitemaps WHERE truncated = 1",
            [],
            |r| r.get(0),
        )?;
        let not_listed: i64 = self.conn().query_row(
            "SELECT count(*) FROM pages p WHERE p.kind = 'html' AND p.noindex = 0 \
             AND p.status >= 200 AND p.status < 300 \
             AND NOT EXISTS (SELECT 1 FROM sitemap_urls s WHERE s.url = p.url)",
            [],
            |r| r.get(0),
        )?;
        let robots: Option<(u16, Option<String>)> = self
            .conn()
            .query_row(
                "SELECT status, body FROM robots_files ORDER BY origin LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;

        Ok(SitemapSummary {
            files: files as u64,
            urls: urls as u64,
            not_crawled: not_crawled as u64,
            not_listed: (truncated == 0).then_some(not_listed as u64),
            robots_status: robots.as_ref().map(|(s, _)| *s),
            robots: robots.and_then(|(_, b)| b),
        })
    }
}
