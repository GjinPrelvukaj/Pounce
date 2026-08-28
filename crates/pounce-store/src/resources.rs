//! Non-page URLs the crawl checked: today, the images pages reference.
//!
//! These live in `resources` and deliberately not in `pages` — migration 011
//! has the reasoning — which is right, and which the Images tab did not know:
//! it filtered `pages` for `kind = 'image'`, a kind the crawler never writes,
//! and so reported "no pages match these filters" over a crawl holding 88
//! images and 24 findings about them.
//!
//! Windowed the same way the grid is, for the same reason. A site with a
//! hundred thousand images is ordinary, and "the UI never receives the
//! dataset" is not a rule about pages, it is a rule about rows.

use crate::schema::{Store, StoreError};

/// One resource row: what was asked for, and what came back.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceRow {
    pub url: String,
    pub status: u16,
    /// `None` when the server declared no length. Not zero — a server that
    /// says nothing is a different report from one that says empty, and
    /// `media.oversized-image` has to read this as *unknown*.
    pub content_length: Option<i64>,
    pub content_type: Option<String>,
    /// Findings whose subject is this URL. Resources have no page id, so this
    /// is the only way the grid can show that a row is the one being
    /// complained about.
    pub issues: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcePage {
    pub rows: Vec<ResourceRow>,
    pub total: u64,
    pub offset: u64,
    pub limit: u32,
}

impl Store {
    /// One window of the resources list, ordered by URL.
    ///
    /// No sort or filter parameters yet: `resources` carries one index, its
    /// primary key, and offering an order it cannot serve is how the grid's
    /// 18-second query happened the first time. URL order is the one this
    /// table is already stored in.
    // ponytail: url order only. Add a sort when there is an index behind it.
    pub fn resource_rows(&self, offset: u64, limit: u32) -> Result<ResourcePage, StoreError> {
        let limit = limit.min(crate::query::MAX_WINDOW);
        let total: i64 = self
            .conn()
            .prepare_cached("SELECT count(*) FROM resources")?
            .query_row([], |r| r.get(0))?;

        let mut stmt = self.conn().prepare_cached(
            "SELECT r.url, r.status, r.content_length, r.content_type, \
                    (SELECT count(*) FROM issues i WHERE i.url = r.url) \
             FROM resources r ORDER BY r.url LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt
            .query_map(
                rusqlite::params![
                    i64::from(limit),
                    // Clamped rather than cast, for the reason `query_rows`
                    // clamps: a negative OFFSET reads as zero in SQLite, so a
                    // stale scroll position would silently show the top of the
                    // list instead of an empty tail.
                    offset.min(i64::MAX as u64) as i64
                ],
                |r| {
                    Ok(ResourceRow {
                        url: r.get(0)?,
                        status: r.get(1)?,
                        content_length: r.get(2)?,
                        content_type: r.get(3)?,
                        issues: r.get::<_, i64>(4)? as u32,
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ResourcePage {
            rows,
            total: total as u64,
            offset,
            limit,
        })
    }
}
