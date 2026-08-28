//! The crawl as a folder tree, one level at a time.

mod common;

use pounce_store::{Store, Writer};

fn seeded(urls: &[&str]) -> Store {
    let mut store = Store::in_memory().unwrap();
    store
        .conn()
        .execute(
            "INSERT INTO crawl (id, seed_url) VALUES (1, 'https://example.com/')",
            [],
        )
        .unwrap();
    {
        let mut writer = Writer::new(&mut store);
        for url in urls {
            writer.push(&common::record_for(url)).unwrap();
        }
        writer.flush().unwrap();
    }
    store
}

const SITE: &[&str] = &[
    "https://example.com/",
    "https://example.com/about",
    "https://example.com/blog/one",
    "https://example.com/blog/two",
    "https://example.com/blog/2026/deep",
    "https://example.com/shop",
];

#[test]
fn the_root_groups_by_first_segment() {
    let store = seeded(SITE);
    let tree = store.site_structure(None).unwrap();
    assert_eq!(tree.prefix, "https://example.com/");

    let names = tree
        .nodes
        .iter()
        .map(|n| (n.segment.as_str(), n.pages, n.folder))
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            // The home page itself: nothing after the prefix.
            ("", 1, false),
            ("about", 1, false),
            // Three pages under it, one of them two levels down. A folder
            // counts everything beneath it, not just its direct children —
            // that is the number that makes a tree worth reading.
            ("blog/", 3, true),
            ("shop", 1, false),
        ]
    );
}

#[test]
fn a_folder_opens_into_its_own_children() {
    let store = seeded(SITE);
    let tree = store
        .site_structure(Some("https://example.com/blog/"))
        .unwrap();
    let names = tree
        .nodes
        .iter()
        .map(|n| (n.segment.as_str(), n.pages))
        .collect::<Vec<_>>();
    assert_eq!(names, vec![("2026/", 1), ("one", 1), ("two", 1)]);
    assert!(
        tree.nodes[0].page_id.is_none(),
        "a folder has nothing to open, even holding one page"
    );
    assert!(
        tree.nodes[1].page_id.is_some(),
        "a single page is the row the tree can open"
    );
}

#[test]
fn a_prefix_matching_nothing_is_an_empty_answer_not_an_error() {
    // The tree asks about a prefix the user typed or a crawl that moved on.
    let store = seeded(SITE);
    let tree = store
        .site_structure(Some("https://example.com/nowhere/"))
        .unwrap();
    assert!(tree.nodes.is_empty());
    assert_eq!(tree.not_listed, 0);
}

#[test]
fn a_sibling_prefix_does_not_leak_in() {
    // `>= 'blog/'` and `< 'blog/\u{10FFFF}'` is the range. Without the upper
    // bound `/blogroll` would answer as a child of `/blog/`.
    let store = seeded(&[
        "https://example.com/blog/one",
        "https://example.com/blogroll",
    ]);
    let tree = store
        .site_structure(Some("https://example.com/blog/"))
        .unwrap();
    assert_eq!(tree.nodes.len(), 1);
    assert_eq!(tree.nodes[0].segment, "one");
}

#[test]
fn a_page_and_the_folder_of_the_same_name_are_one_row() {
    // `/blog` and `/blog/one` both exist on most sites. Listed separately they
    // read as a duplicate — the same word twice, one with a slash — so the
    // folder takes the row and carries its own page's id.
    let store = seeded(&[
        "https://example.com/blog",
        "https://example.com/blog/one",
        "https://example.com/blog/two",
    ]);
    let tree = store.site_structure(None).unwrap();
    assert_eq!(tree.nodes.len(), 1, "one row, not two");

    let blog = &tree.nodes[0];
    assert_eq!(blog.segment, "blog/");
    assert!(blog.folder);
    assert_eq!(blog.pages, 3, "the folder counts its own page too");
    assert!(
        blog.page_id.is_some(),
        "the folder has a page at its own address, and the tree can open it"
    );
}

#[test]
fn the_root_follows_the_redirect_the_crawl_followed() {
    // Seeded at the apex, landed on `www` — the shape of most real sites, and
    // the one that showed a root line with nothing under it. The tree has to
    // root where the pages are, not where the crawl was pointed.
    let mut store = Store::in_memory().unwrap();
    store
        .conn()
        .execute(
            "INSERT INTO crawl (id, seed_url) VALUES (1, 'https://example.com/')",
            [],
        )
        .unwrap();
    {
        let mut writer = Writer::new(&mut store);
        for url in [
            "https://www.example.com/",
            "https://www.example.com/about",
            "https://www.example.com/blog/one",
        ] {
            writer.push(&common::record_for(url)).unwrap();
        }
        writer.flush().unwrap();
    }

    let tree = store.site_structure(None).unwrap();
    assert_eq!(tree.prefix, "https://www.example.com/");
    assert_eq!(
        tree.nodes.len(),
        3,
        "the home page, /about and the blog folder"
    );
}

#[test]
fn a_crawl_with_no_pages_still_answers() {
    // Before the first row lands, and after a crawl that found nothing. The
    // seed is all there is to say, and saying it beats an error.
    let store = seeded(&[]);
    let tree = store.site_structure(None).unwrap();
    assert_eq!(tree.prefix, "https://example.com/");
    assert!(tree.nodes.is_empty());
}
