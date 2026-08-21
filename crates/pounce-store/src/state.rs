//! Durable crawl identity and frontier state.

use crate::{Store, StoreError};
use pounce_core::{CrawlLimits, CrawlUrl};
use rusqlite::{OptionalExtension, params};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontierEntry {
    pub url: CrawlUrl,
    pub depth: u16,
    pub done: bool,
}

pub struct CrawlState<'a> {
    store: &'a mut Store,
}

impl<'a> CrawlState<'a> {
    pub fn new(store: &'a mut Store) -> Self {
        Self { store }
    }

    pub fn start(&mut self, seed: &CrawlUrl) -> Result<(), StoreError> {
        self.start_with_limits(seed, CrawlLimits::default())
    }

    pub fn start_with_limits(
        &mut self,
        seed: &CrawlUrl,
        limits: CrawlLimits,
    ) -> Result<(), StoreError> {
        let max_urls = limits
            .max_urls
            .map(|value| i64::try_from(value).map_err(|_| StoreError::LimitTooLarge("max_urls")))
            .transpose()?;
        let max_duration_ns = limits
            .max_duration
            .map(|value| {
                i64::try_from(value.as_nanos())
                    .map_err(|_| StoreError::LimitTooLarge("max_duration"))
            })
            .transpose()?;
        let tx = self.store.conn_mut().transaction()?;
        tx.execute(
            "INSERT INTO crawl \
             (id, seed_url, max_depth, max_urls, max_duration_ns) \
             VALUES (1, ?1, ?2, ?3, ?4)",
            params![
                seed.to_string(),
                limits.max_depth,
                max_urls,
                max_duration_ns
            ],
        )?;
        tx.execute(
            "INSERT INTO frontier (url, depth) VALUES (?1, 0)",
            [seed.to_string()],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn limits(&self) -> Result<Option<CrawlLimits>, StoreError> {
        self.store
            .conn()
            .query_row(
                "SELECT max_depth, max_urls, max_duration_ns FROM crawl WHERE id = 1",
                [],
                |row| {
                    Ok(CrawlLimits {
                        max_depth: row.get(0)?,
                        max_urls: row.get(1)?,
                        max_duration: row
                            .get::<_, Option<u64>>(2)?
                            .map(std::time::Duration::from_nanos),
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn seed(&self) -> Result<Option<CrawlUrl>, StoreError> {
        let raw = self
            .store
            .conn()
            .query_row("SELECT seed_url FROM crawl WHERE id = 1", [], |row| {
                row.get::<_, String>(0)
            })
            .optional()?;
        raw.map(parse_url).transpose()
    }

    pub fn discover(&mut self, entries: &[(CrawlUrl, u16)]) -> Result<(), StoreError> {
        let tx = self.store.conn_mut().transaction()?;
        insert_frontier(&tx, entries)?;
        tx.commit()?;
        Ok(())
    }

    pub fn load(&self) -> Result<Vec<FrontierEntry>, StoreError> {
        let mut stmt = self.store.conn().prepare(
            "SELECT f.url, f.depth, p.id IS NOT NULL OR e.url IS NOT NULL \
                 FROM frontier f LEFT JOIN pages p ON p.url = f.url \
                 LEFT JOIN crawl_failures e ON e.url = f.url \
                 ORDER BY f.depth, f.url",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u16>(1)?,
                    row.get::<_, bool>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(url, depth, done)| {
                Ok(FrontierEntry {
                    url: parse_url(url)?,
                    depth,
                    done,
                })
            })
            .collect()
    }
}

fn parse_url(url: String) -> Result<CrawlUrl, StoreError> {
    CrawlUrl::parse(&url).map_err(|source| StoreError::InvalidUrl { url, source })
}

pub(crate) fn insert_frontier(
    conn: &rusqlite::Connection,
    entries: &[(CrawlUrl, u16)],
) -> Result<(), StoreError> {
    let mut stmt = conn.prepare_cached(
        "INSERT INTO frontier (url, depth) VALUES (?1, ?2) \
         ON CONFLICT(url) DO UPDATE SET \
         depth = min(frontier.depth, excluded.depth)",
    )?;
    for (url, depth) in entries {
        stmt.execute(params![url.to_string(), depth])?;
    }
    Ok(())
}
