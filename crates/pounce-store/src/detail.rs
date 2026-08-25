//! Everything about one page, for the detail pane.
//!
//! The opposite end of the design from `query_rows`: that one reads nine narrow
//! columns from a million rows, this one reads every column from exactly one —
//! plus the two directions of the link graph and the findings against it. Which
//! is why the six JSON fields live in `page_detail` (migration 012): they are
//! read here, one row at a time, and never sorted or grouped.
//!
//! The link lists are **capped and counted separately**. A hub page on a large
//! site has tens of thousands of inlinks; sending them all would break the one
//! invariant this crate exists to protect, and nobody reads past the first
//! screenful. The count is the honest number, the list is what fits.

use crate::schema::{Store, StoreError};
use serde::Serialize;
use serde_json::Value;

/// How many links of each direction the pane receives.
///
/// Not a preference: this is the "UI never receives the dataset" invariant
/// applied to a page rather than to a crawl. `inlink_count` carries the truth
/// the list is a sample of.
pub const MAX_LINKS: usize = 100;

/// One edge, in whichever direction it was asked for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkRow {
    /// The page at the other end: the linking page for an inlink, the linked
    /// URL for an outlink.
    pub url: String,
    /// Empty for an image-only link, which is a finding rather than a blank.
    pub anchor_text: String,
    pub nofollow: bool,
    /// Whether that URL is a page in this crawl. `false` for an outlink that
    /// left the scope, or that was never reached.
    pub crawled: bool,
}

/// One finding against this page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailIssue {
    pub rule_id: String,
    pub severity: String,
    pub detail: Option<String>,
}

/// Everything the pane shows about one page.
///
/// The five repeating fields cross as parsed JSON rather than as strings: they
/// are stored as JSON text and the alternative is a `JSON.parse` per field in
/// the UI, which is the same work done later and with no type behind it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageDetail {
    pub id: i64,
    pub url: String,
    pub status: u16,
    pub depth: u16,
    pub size: i64,
    pub truncated: bool,
    pub content_type: Option<String>,
    pub charset: Option<String>,
    pub kind: String,
    pub content_type_mismatch: bool,
    pub elapsed_ms: i64,
    pub time_to_headers_ms: i64,
    pub title: Option<String>,
    pub meta_description: Option<String>,
    pub canonical: Option<String>,
    pub canonical_url: Option<String>,
    pub noindex: bool,
    pub nofollow: bool,
    pub noarchive: bool,
    pub nosnippet: bool,
    pub word_count: u32,
    /// URLs crossed to reach this page, in order. Empty for a direct hit — the
    /// chain is a finding in its own right, not routing detail.
    pub redirect_chain: Value,
    pub h1: Value,
    pub h2: Value,
    pub hreflang: Value,
    pub open_graph: Value,
    pub images: Value,
    pub issues: Vec<DetailIssue>,
    /// Every page linking here, and every page this one links to — capped at
    /// [`MAX_LINKS`] each, with the true totals beside them.
    pub inlinks: Vec<LinkRow>,
    pub outlinks: Vec<LinkRow>,
    pub inlink_count: u64,
    pub outlink_count: u64,
}

/// Parses a stored JSON column, falling back to `null` rather than failing.
///
/// A file whose JSON a future build cannot read should still open: one
/// unreadable heading list is not a reason to refuse the whole page, and the
/// pane draws nothing for a null.
fn json(raw: String) -> Value {
    serde_json::from_str(&raw).unwrap_or(Value::Null)
}

impl Store {
    /// Everything about one page, or `None` if that id is not in this file.
    pub fn page_detail(&self, id: i64) -> Result<Option<PageDetail>, StoreError> {
        let row = self.conn().query_row(
            "SELECT p.url, p.status, p.depth, p.size, p.truncated, p.content_type, p.charset, \
                    p.kind, p.content_type_mismatch, p.elapsed_ms, p.time_to_headers_ms, \
                    p.title, p.meta_description, p.canonical, p.canonical_url, \
                    p.noindex, p.nofollow, p.noarchive, p.nosnippet, p.word_count, \
                    d.redirect_chain, d.h1, d.h2, d.hreflang, d.open_graph, d.images \
             FROM pages p LEFT JOIN page_detail d ON d.page_id = p.id WHERE p.id = ?1",
            [id],
            |r| {
                Ok(PageDetail {
                    id,
                    url: r.get(0)?,
                    status: r.get::<_, i64>(1)? as u16,
                    depth: r.get::<_, i64>(2)? as u16,
                    size: r.get(3)?,
                    truncated: r.get::<_, i64>(4)? != 0,
                    content_type: r.get(5)?,
                    charset: r.get(6)?,
                    kind: r.get(7)?,
                    content_type_mismatch: r.get::<_, i64>(8)? != 0,
                    elapsed_ms: r.get(9)?,
                    time_to_headers_ms: r.get(10)?,
                    title: r.get(11)?,
                    meta_description: r.get(12)?,
                    canonical: r.get(13)?,
                    canonical_url: r.get(14)?,
                    noindex: r.get::<_, i64>(15)? != 0,
                    nofollow: r.get::<_, i64>(16)? != 0,
                    noarchive: r.get::<_, i64>(17)? != 0,
                    nosnippet: r.get::<_, i64>(18)? != 0,
                    word_count: r.get::<_, i64>(19)? as u32,
                    redirect_chain: json(r.get(20).unwrap_or_default()),
                    h1: json(r.get(21).unwrap_or_default()),
                    h2: json(r.get(22).unwrap_or_default()),
                    hreflang: json(r.get(23).unwrap_or_default()),
                    open_graph: json(r.get(24).unwrap_or_default()),
                    images: json(r.get(25).unwrap_or_default()),
                    issues: Vec::new(),
                    inlinks: Vec::new(),
                    outlinks: Vec::new(),
                    inlink_count: 0,
                    outlink_count: 0,
                })
            },
        );
        let mut detail = match row {
            Ok(detail) => detail,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
            Err(e) => return Err(e.into()),
        };

        {
            let mut stmt = self.conn().prepare_cached(
                "SELECT rule_id, severity, detail FROM issues WHERE page_id = ?1 \
                 ORDER BY severity, rule_id",
            )?;
            let rows = stmt.query_map([id], |r| {
                Ok(DetailIssue {
                    rule_id: r.get(0)?,
                    severity: r.get(1)?,
                    detail: r.get(2)?,
                })
            })?;
            for row in rows {
                detail.issues.push(row?);
            }
        }

        // Outlinks: the anchors on this page. `target_url IS NULL` is a
        // `mailto:` or a malformed href — still a link the page has, so it is
        // shown as written rather than dropped.
        {
            let mut stmt = self.conn().prepare_cached(
                "SELECT coalesce(l.target_url, l.href), l.anchor_text, l.nofollow, \
                        l.target_url IS NOT NULL AND p.id IS NOT NULL \
                 FROM links l LEFT JOIN pages p ON p.url = l.target_url \
                 WHERE l.source_page_id = ?1 ORDER BY l.id LIMIT ?2",
            )?;
            detail.outlinks = link_rows(&mut stmt, [id, MAX_LINKS as i64])?;
        }
        detail.outlink_count = self.conn().query_row(
            "SELECT count(*) FROM links WHERE source_page_id = ?1",
            [id],
            |r| r.get::<_, i64>(0),
        )? as u64;

        // Inlinks: every anchor anywhere in the crawl pointing at this URL.
        // Served by `links_target`, which `build_query_indices` defers to the
        // end of the crawl — during a live crawl this is a scan, which is why
        // the pane asks for a capped list rather than a count first.
        {
            let mut stmt = self.conn().prepare_cached(
                "SELECT p.url, l.anchor_text, l.nofollow, 1 \
                 FROM links l JOIN pages p ON p.id = l.source_page_id \
                 WHERE l.target_url = ?1 ORDER BY l.id LIMIT ?2",
            )?;
            let url = detail.url.clone();
            let rows = stmt.query_map(rusqlite::params![url, MAX_LINKS as i64], |r| {
                Ok(LinkRow {
                    url: r.get(0)?,
                    anchor_text: r.get(1)?,
                    nofollow: r.get::<_, i64>(2)? != 0,
                    crawled: r.get::<_, i64>(3)? != 0,
                })
            })?;
            for row in rows {
                detail.inlinks.push(row?);
            }
        }
        detail.inlink_count = self.conn().query_row(
            "SELECT count(*) FROM links WHERE target_url = ?1",
            [&detail.url],
            |r| r.get::<_, i64>(0),
        )? as u64;

        Ok(Some(detail))
    }
}

fn link_rows(
    stmt: &mut rusqlite::CachedStatement<'_>,
    params: [i64; 2],
) -> Result<Vec<LinkRow>, StoreError> {
    let rows = stmt.query_map(params, |r| {
        Ok(LinkRow {
            url: r.get(0)?,
            anchor_text: r.get(1)?,
            nofollow: r.get::<_, i64>(2)? != 0,
            crawled: r.get::<_, i64>(3)? != 0,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}
