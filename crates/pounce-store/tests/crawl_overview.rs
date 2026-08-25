//! What a crawl is made of — the counts behind the overview panel.

mod common;

use pounce_store::Store;

#[test]
fn the_overview_counts_what_the_crawl_contains() {
    // The seeded shape: ~90% 200s with 404s and 500s mixed in, a PDF every
    // 25th page and an image every 40th, and every tenth page noindex.
    let store = common::seeded(200);
    let overview = store.crawl_overview().unwrap();

    assert_eq!(overview.crawled, 200);
    assert_eq!(overview.indexable + overview.noindex, 200);
    assert_eq!(overview.noindex, 20, "every tenth page");

    // Kinds add up to the whole crawl, and are ordered for the panel rather
    // than alphabetically — HTML first, whatever a site happens to contain.
    let total: u64 = overview.by_kind.iter().map(|(_, n)| n).sum();
    assert_eq!(total, 200);
    assert_eq!(overview.by_kind[0].0, "html");

    // Response classes are the crawl's own, not a guess: 200 pages, every
    // 50th a 500 and every 20th a 404.
    assert_eq!(overview.by_class[4], 4, "500s: every 50th of 200");
    // Multiples of 20 up to 200 are ten, less the two that are also multiples
    // of 50 and so were served as 500s.
    assert_eq!(overview.by_class[3], 8, "404s");
    assert_eq!(overview.by_class.iter().sum::<u64>(), 200);
}

#[test]
fn an_empty_crawl_answers_with_zeroes_rather_than_an_error() {
    // The panel is drawn before the first batch commits, and a file with no
    // rows yet is the ordinary case rather than a failure.
    let store = Store::in_memory().unwrap();
    let overview = store.crawl_overview().unwrap();
    assert_eq!(overview.crawled, 0);
    assert_eq!(overview.queued, 0);
    assert!(overview.by_kind.is_empty());
    assert_eq!(overview.by_class, [0; 5]);
}
