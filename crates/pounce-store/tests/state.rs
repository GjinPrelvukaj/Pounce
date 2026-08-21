//! Persistence and restart behavior for the crawl frontier.

use pounce_core::CrawlUrl;
use pounce_parse::{BodyKind, MetaRobots, PageRecord};
use pounce_store::{CrawlState, Store, StoreError, Writer};
use std::collections::HashSet;

fn url(i: usize) -> CrawlUrl {
    CrawlUrl::parse(&format!("https://example.com/{i}")).unwrap()
}

fn record(url: CrawlUrl) -> PageRecord {
    PageRecord {
        url,
        status: 200,
        depth: 1,
        size: 0,
        truncated: false,
        content_type: Some("text/html".into()),
        charset: Some("utf-8".into()),
        kind: BodyKind::Html,
        content_type_mismatch: false,
        elapsed_ms: 1,
        time_to_headers_ms: 1,
        redirect_chain: vec![],
        title: None,
        meta_description: None,
        h1: vec![],
        h2: vec![],
        canonical: None,
        canonical_url: None,
        meta_robots: MetaRobots::default(),
        hreflang: vec![],
        open_graph: vec![],
        links: vec![],
        images: vec![],
        word_count: 0,
    }
}

#[test]
fn crawl_identity_and_discoveries_survive_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("resume.pounce");
    let seed = url(0);
    {
        let mut store = Store::open(&path).unwrap();
        let mut state = CrawlState::new(&mut store);
        state.start(&seed).unwrap();
        state
            .discover(&[(url(1), 3), (url(1), 1), (url(2), 2)])
            .unwrap();
    }

    let mut reopened = Store::open(&path).unwrap();
    let state = CrawlState::new(&mut reopened);
    assert_eq!(state.seed().unwrap(), Some(seed));
    let entries = state.load().unwrap();
    assert_eq!(entries.len(), 3, "the seed plus two unique discoveries");
    assert_eq!(entries[0].depth, 0);
    assert_eq!(
        entries[1].depth, 1,
        "rediscovery keeps the shallowest depth"
    );
    assert!(entries.iter().all(|entry| !entry.done));
}

#[test]
fn a_hand_edited_invalid_frontier_url_is_rejected() {
    let mut store = Store::in_memory().unwrap();
    store
        .conn()
        .execute(
            "INSERT INTO frontier (url, depth) VALUES ('file:///tmp/secret', 0)",
            [],
        )
        .unwrap();

    let state = CrawlState::new(&mut store);
    assert!(matches!(
        state.load(),
        Err(StoreError::InvalidUrl { url, .. }) if url == "file:///tmp/secret"
    ));
}

#[test]
fn page_and_completion_roll_back_together() {
    let mut store = Store::in_memory().unwrap();
    let seed = url(0);
    {
        let mut state = CrawlState::new(&mut store);
        state.start(&seed).unwrap();
    }
    {
        let mut writer = Writer::with_batch_size(&mut store, 2);
        writer.push(&record(seed.clone())).unwrap();
        // Dropping an open batch simulates a killed crawl.
    }

    let entry = {
        let state = CrawlState::new(&mut store);
        state
            .load()
            .unwrap()
            .into_iter()
            .find(|entry| entry.url == seed)
            .unwrap()
    };
    assert!(!entry.done);
    assert_eq!(
        store
            .conn()
            .query_row("SELECT count(*) FROM pages", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn writer_discoveries_share_the_page_batch_commit() {
    let mut store = Store::in_memory().unwrap();
    let seed = url(0);
    let target = url(1);
    {
        let mut state = CrawlState::new(&mut store);
        state.start(&seed).unwrap();
    }
    {
        let mut writer = Writer::with_batch_size(&mut store, 2);
        writer.discover(&[(target.clone(), 1)]).unwrap();
        writer.push(&record(seed.clone())).unwrap();
        // Neither the page nor its discovery commits before the batch does.
    }
    {
        let state = CrawlState::new(&mut store);
        assert_eq!(state.load().unwrap().len(), 1);
    }

    {
        let mut writer = Writer::with_batch_size(&mut store, 1);
        writer.discover(&[(target.clone(), 1)]).unwrap();
        writer.push(&record(seed)).unwrap();
    }
    let state = CrawlState::new(&mut store);
    assert!(
        state
            .load()
            .unwrap()
            .iter()
            .any(|entry| entry.url == target)
    );
}

#[test]
fn terminal_failure_and_completion_share_the_batch_commit() {
    let mut store = Store::in_memory().unwrap();
    let seed = url(0);
    {
        CrawlState::new(&mut store).start(&seed).unwrap();
    }
    {
        let mut writer = Writer::with_batch_size(&mut store, 2);
        writer.fail(&seed, "connection refused").unwrap();
        // A killed open batch must leave this URL pending.
    }
    assert!(!CrawlState::new(&mut store).load().unwrap()[0].done);

    {
        let mut writer = Writer::with_batch_size(&mut store, 1);
        writer.fail(&seed, "connection refused").unwrap();
    }
    assert!(CrawlState::new(&mut store).load().unwrap()[0].done);
    assert_eq!(
        store
            .conn()
            .query_row(
                "SELECT reason FROM crawl_failures WHERE url = ?1",
                [seed.to_string()],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "connection refused"
    );
}

#[test]
fn a_crawl_killed_after_50k_resumes_without_duplicates_or_losses() {
    const TOTAL: usize = 100_000;
    const ATTEMPTED_BEFORE_KILL: usize = 50_250;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("resume-100k.pounce");
    let urls = (0..TOTAL).map(url).collect::<Vec<_>>();
    {
        let mut store = Store::open(&path).unwrap();
        {
            let mut state = CrawlState::new(&mut store);
            state.start(&urls[0]).unwrap();
            let discovered = urls[1..]
                .iter()
                .cloned()
                .map(|url| (url, 1))
                .collect::<Vec<_>>();
            state.discover(&discovered).unwrap();
        }
        let mut writer = Writer::with_batch_size(&mut store, 500);
        for url in &urls[..ATTEMPTED_BEFORE_KILL] {
            writer.push(&record(url.clone())).unwrap();
        }
        assert_eq!(writer.committed(), 50_000);
        // The final 250 rows and their completion bits roll back on drop.
    }

    let pending = {
        let mut store = Store::open(&path).unwrap();
        let state = CrawlState::new(&mut store);
        let entries = state.load().unwrap();
        assert_eq!(entries.len(), TOTAL);
        assert_eq!(
            entries
                .iter()
                .map(|entry| &entry.url)
                .collect::<HashSet<_>>()
                .len(),
            TOTAL
        );
        assert_eq!(entries.iter().filter(|entry| entry.done).count(), 50_000);
        entries
            .into_iter()
            .filter(|entry| !entry.done)
            .map(|entry| entry.url)
            .collect::<Vec<_>>()
    };
    assert_eq!(pending.len(), 50_000);

    {
        let mut store = Store::open(&path).unwrap();
        let mut writer = Writer::with_batch_size(&mut store, 500);
        for url in pending {
            writer.push(&record(url)).unwrap();
        }
        writer.flush().unwrap();
    }

    let mut store = Store::open(&path).unwrap();
    {
        let state = CrawlState::new(&mut store);
        assert!(state.load().unwrap().iter().all(|entry| entry.done));
    }
    assert_eq!(
        store
            .conn()
            .query_row("SELECT count(*) FROM pages", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        TOTAL as i64
    );
}
