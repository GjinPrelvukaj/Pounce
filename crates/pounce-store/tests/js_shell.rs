//! Spotting a site this build cannot read properly.

mod common;

use pounce_store::{Store, Writer};

/// A page with `words` words and `bytes` of markup.
fn page(store: &mut Store, path: &str, words: u32, bytes: usize) {
    let mut record = common::record_for(&format!("https://example.com/{path}"));
    record.word_count = words;
    record.size = bytes;
    let mut writer = Writer::new(store);
    writer.push(&record).unwrap();
    writer.flush().unwrap();
}

#[test]
fn a_page_of_markup_with_no_words_in_it_is_counted() {
    // The shape of a client-rendered app: the framework ships its containers
    // and the text arrives later, from JavaScript this build does not run.
    let mut store = Store::in_memory().unwrap();
    page(&mut store, "shell", 30, 45_000);
    assert_eq!(store.crawl_overview().unwrap().js_shell, 1);
}

#[test]
fn a_real_page_is_not_counted_however_large() {
    let mut store = Store::in_memory().unwrap();
    page(&mut store, "long", 900, 45_000);
    assert_eq!(store.crawl_overview().unwrap().js_shell, 0);
}

#[test]
fn a_genuinely_small_page_is_not_counted() {
    // A stub, a thin landing page, a 404 body. Few words *and* few bytes is
    // not the signal — the signal is the ratio between them.
    let mut store = Store::in_memory().unwrap();
    page(&mut store, "stub", 12, 900);
    assert_eq!(store.crawl_overview().unwrap().js_shell, 0);
}

#[test]
fn the_boundaries_are_where_they_are_declared() {
    // Break either threshold on purpose and the count has to move, or the
    // numbers are decoration.
    use pounce_store::query::{JS_SHELL_MAX_WORDS, JS_SHELL_MIN_BYTES};
    let mut store = Store::in_memory().unwrap();
    page(
        &mut store,
        "just-under",
        JS_SHELL_MAX_WORDS - 1,
        JS_SHELL_MIN_BYTES as usize + 1,
    );
    page(
        &mut store,
        "at-the-word-cap",
        JS_SHELL_MAX_WORDS,
        JS_SHELL_MIN_BYTES as usize + 1,
    );
    page(
        &mut store,
        "at-the-byte-floor",
        1,
        JS_SHELL_MIN_BYTES as usize,
    );
    assert_eq!(
        store.crawl_overview().unwrap().js_shell,
        1,
        "only the page inside both bounds counts"
    );
}
