//! The crawl as a folder tree.
//!
//! Screaming Frog's tree view, and the thing a client actually recognises: a
//! list of 4,000 URLs says nothing about a site, and `/baseball/new-jersey/`
//! holding 3,900 of them says most of it.
//!
//! One level at a time, by prefix. The invariant is unchanged — the UI never
//! receives the dataset, it asks for the children of one folder and gets a
//! grouped count. A tree built in the browser from every URL is the same
//! mistake as a grid built from every row, with an extra recursion.

use crate::schema::{Store, StoreError};
use rusqlite::OptionalExtension;

/// One child of a folder: a subfolder, or a page.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructureNode {
    /// The segment as it reads in the URL. A folder keeps its trailing slash,
    /// which is what tells it from a page — `/blog/` and `/blog` are different
    /// things to a server and this view does not pretend otherwise.
    pub segment: String,
    /// The full prefix, for asking about this node's own children.
    pub prefix: String,
    pub folder: bool,
    pub pages: u64,
    /// Pages beneath here with at least one finding. Reads `has_issue`, the
    /// cache migration 013 keeps in step with `issues`.
    pub with_issues: u64,
    /// The page id, when this node *is* one page. `None` for a folder, so the
    /// tree opens a detail pane only where there is one page to open.
    pub page_id: Option<i64>,
}

/// The children of one folder.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Structure {
    /// The prefix these are children of. Echoed back because the caller may
    /// have passed `None` and let the store find the crawl's own root.
    pub prefix: String,
    pub nodes: Vec<StructureNode>,
    /// Children not in `nodes`. Never silently dropped: a folder with 20,000
    /// direct children says so rather than showing 500 and looking complete.
    pub not_listed: u64,
}

/// The most children one call returns.
pub const MAX_CHILDREN: u32 = 500;

impl Store {
    /// The children of `prefix`, grouped by their next path segment.
    ///
    /// `None` asks for the crawl's root, which is the seed's origin. A crawl
    /// that reached a second host shows only the seed's — scope keeps crawls
    /// to one site by default, and a two-rooted tree for the exception is a
    /// feature nobody has asked for yet.
    // ponytail: single root. Group by origin when a multi-host crawl exists.
    pub fn site_structure(&self, prefix: Option<&str>) -> Result<Structure, StoreError> {
        let prefix = match prefix {
            Some(p) => p.to_string(),
            None => self.crawl_root()?,
        };
        // A range rather than `LIKE 'prefix%'`: SQLite only uses an index for
        // LIKE when the operator's case sensitivity matches the index's, and
        // the default does not. `>=` and `<` always use `pages.url`, which is
        // the UNIQUE index every crawl already carries.
        let upper = format!("{prefix}\u{10FFFF}");
        let n = prefix.chars().count() as i64;

        // `substr` counts characters, and stored URLs are canonical — every
        // non-ASCII byte is already percent-encoded — so characters and bytes
        // agree here. A raw needle would not; see the search gotcha.
        // Grouped on the name *without* its trailing slash, so `/baseball` and
        // `/baseball/...` are one row rather than two that read as a
        // duplicate. The folder wins the row and carries its own page's id;
        // the page itself is offered as the first child when it is opened.
        let sql = "\
            WITH kids AS ( \
                SELECT CASE WHEN instr(substr(url, ?1 + 1), '/') > 0 \
                            THEN substr(url, ?1 + 1, instr(substr(url, ?1 + 1), '/')) \
                            ELSE substr(url, ?1 + 1) END AS seg, \
                       has_issue, id \
                FROM pages WHERE url >= ?2 AND url < ?3 \
            ) \
            SELECT rtrim(seg, '/') AS name, max(seg LIKE '%/'), count(*), \
                   sum(has_issue), min(CASE WHEN seg NOT LIKE '%/' THEN id END) \
            FROM kids GROUP BY name ORDER BY name";

        let mut stmt = self.conn().prepare_cached(sql)?;
        let rows = stmt
            .query_map(rusqlite::params![n, &prefix, &upper], |r| {
                let name: String = r.get(0)?;
                let folder = r.get::<_, i64>(1)? != 0;
                let pages: i64 = r.get(2)?;
                let with_issues: i64 = r.get::<_, Option<i64>>(3)?.unwrap_or(0);
                // The id of the page at this exact address, when there is one.
                // A folder can have one — `/blog` and `/blog/post` both exist
                // on most sites — and then the row both opens and expands.
                let page_id: Option<i64> = r.get(4)?;
                let segment = if folder { format!("{name}/") } else { name };
                Ok(StructureNode {
                    prefix: format!("{prefix}{segment}"),
                    page_id,
                    segment,
                    folder,
                    pages: pages as u64,
                    with_issues: with_issues as u64,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let total = rows.len() as u64;
        let mut nodes = rows;
        nodes.truncate(MAX_CHILDREN as usize);
        Ok(Structure {
            not_listed: total - nodes.len() as u64,
            prefix,
            nodes,
        })
    }

    /// The origin the crawl's pages actually live under, with its trailing
    /// slash: `https://example.com/`.
    ///
    /// **Read from the pages, not from the seed.** The seed is where the crawl
    /// was pointed; it is not necessarily where anything was found. Seeding
    /// `https://myzion.com/` on a site that redirects the apex to `www` stores
    /// eighteen pages under `https://www.myzion.com/` and not one under the
    /// seed — so a tree rooted at the seed showed the root line and no
    /// children at all. Every crawl of a site with a canonical host redirect
    /// hit this, which is most of them.
    ///
    /// The shallowest URL is the home page by construction: depth is distance
    /// from the seed in links followed, so depth 0 is where the crawl actually
    /// landed after redirects. The seed is the fallback for a crawl with no
    /// pages, where there is nothing else to say.
    fn crawl_root(&self) -> Result<String, StoreError> {
        // `min(depth)` uses the depth index, and the equality that follows
        // uses it again — this is not a scan of a million rows to find one.
        let landed: Option<String> = self
            .conn()
            .query_row(
                "SELECT url FROM pages WHERE depth = (SELECT min(depth) FROM pages) \
                 ORDER BY length(url), url LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let from = match landed {
            Some(url) => url,
            None => self
                .conn()
                .query_row("SELECT seed_url FROM crawl WHERE id = 1", [], |r| r.get(0))
                .optional()?
                .unwrap_or_default(),
        };
        // Third slash: `https://host/…`. Everything up to and including it is
        // the origin, and a URL with no path at all still ends there.
        let after_scheme = from.find("//").map(|i| i + 2).unwrap_or(0);
        Ok(match from[after_scheme..].find('/') {
            Some(i) => from[..after_scheme + i + 1].to_string(),
            None => format!("{from}/"),
        })
    }
}
