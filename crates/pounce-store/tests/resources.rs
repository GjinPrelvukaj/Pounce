//! The resources window: images and other non-page URLs the crawl checked.

use pounce_core::CrawlUrl;
use pounce_store::{Store, Writer};

fn seeded(n: u32) -> Store {
    let mut store = Store::in_memory().unwrap();
    {
        let mut writer = Writer::new(&mut store);
        for i in 0..n {
            writer
                .resource(
                    &CrawlUrl::parse(&format!("https://example.com/img/{i:04}.jpg")).unwrap(),
                    200,
                    Some(1_000 + u64::from(i)),
                    Some("image/jpeg"),
                )
                .unwrap();
        }
        writer.flush().unwrap();
    }
    store
}

#[test]
fn the_window_is_a_window_and_the_total_is_the_total() {
    let store = seeded(50);
    let page = store.resource_rows(10, 5).unwrap();
    assert_eq!(page.total, 50, "the total is the whole table");
    assert_eq!(page.rows.len(), 5);
    assert!(page.rows[0].url.ends_with("0010.jpg"), "offset is honoured");
}

#[test]
fn an_absurd_offset_returns_nothing_rather_than_wrapping() {
    // The same hazard `query_rows` clamps for: a scroll position computed from
    // a stale total, cast to a negative, read by SQLite as zero.
    let store = seeded(10);
    for offset in [u64::MAX, i64::MAX as u64 + 1] {
        let page = store.resource_rows(offset, 10).unwrap();
        assert!(page.rows.is_empty(), "offset {offset} wrapped");
        assert_eq!(page.total, 10);
    }
}

#[test]
fn a_declared_length_of_nothing_is_not_a_length_of_zero() {
    // `oversized-image` has to read absent as *unknown*, so this distinction
    // has to survive all the way to the grid.
    let mut store = Store::in_memory().unwrap();
    {
        let mut writer = Writer::new(&mut store);
        writer
            .resource(
                &CrawlUrl::parse("https://example.com/a.png").unwrap(),
                200,
                None,
                Some("image/png"),
            )
            .unwrap();
        writer
            .resource(
                &CrawlUrl::parse("https://example.com/b.png").unwrap(),
                200,
                Some(0),
                Some("image/png"),
            )
            .unwrap();
        writer.flush().unwrap();
    }
    let rows = store.resource_rows(0, 10).unwrap().rows;
    assert_eq!(rows[0].content_length, None, "absent stays absent");
    assert_eq!(rows[1].content_length, Some(0), "zero stays zero");
}
