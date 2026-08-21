//! Durable crawl identity and frontier state.

use crate::{Store, StoreError};
use pounce_core::CrawlUrl;
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
        let tx = self.store.conn_mut().transaction()?;
        tx.execute(
            "INSERT INTO crawl (id, seed_url) VALUES (1, ?1)",
            [seed.to_string()],
        )?;
        tx.execute(
            "INSERT INTO frontier (url, depth) VALUES (?1, 0)",
            [seed.to_string()],
        )?;
        tx.commit()?;
        Ok(())
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
