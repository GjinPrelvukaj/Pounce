//! What the extractor reads out of markup.
//!
//! Two layers. Hand-written expectations for each field, which is where the
//! real assertions live, and a golden-file snapshot of the whole record over a
//! committed corpus, which is what catches an unintended change to a field
//! nobody was thinking about. The corpus deliberately includes markup no
//! browser would call valid: the pages most in need of an SEO audit are the
//! broken ones, and a parser that gives up on them is useless.
//!
//! Regenerate the goldens with `UPDATE_GOLDEN=1 cargo test -p pounce-parse`,
//! and read the diff before committing it. A golden accepted without reading
//! is a test that asserts whatever the code happens to do.

use pounce_core::CrawlUrl;
use pounce_parse::{MetaRobots, PageRecord, extract};
use std::path::{Path, PathBuf};

const BASE: &str = "https://example.com/shop/index.html";

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus")
}

/// A record with the transport half filled in by hand, so these tests exercise
/// extraction alone and need no server.
fn blank(url: &str) -> PageRecord {
    PageRecord {
        url: CrawlUrl::parse(url).unwrap(),
        status: 200,
        depth: 0,
        size: 0,
        truncated: false,
        content_type: Some("text/html".into()),
        charset: Some("utf-8".into()),
        kind: pounce_parse::BodyKind::Html,
        content_type_mismatch: false,
        elapsed_ms: 0,
        time_to_headers_ms: 0,
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

fn parse_file(name: &str) -> PageRecord {
    let html = std::fs::read(corpus_dir().join(name)).unwrap();
    let mut record = blank(BASE);
    extract(&mut record, &html).unwrap();
    record
}

fn parse_str(html: &str) -> PageRecord {
    let mut record = blank(BASE);
    extract(&mut record, html.as_bytes()).unwrap();
    record
}

// ---- golden files --------------------------------------------------------

#[test]
fn the_corpus_matches_its_goldens() {
    let mut checked = 0;
    let mut entries: Vec<_> = std::fs::read_dir(corpus_dir())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".html"))
        .collect();
    entries.sort();

    for name in entries {
        let record = parse_file(&name);
        let actual = serde_json::to_string_pretty(&record).unwrap() + "\n";
        let golden = corpus_dir().join(name.replace(".html", ".json"));

        if std::env::var("UPDATE_GOLDEN").is_ok() {
            std::fs::write(&golden, &actual).unwrap();
            continue;
        }
        let expected = std::fs::read_to_string(&golden)
            .unwrap_or_else(|_| panic!("missing golden for {name}; run with UPDATE_GOLDEN=1"));
        assert_eq!(expected, actual, "{name} changed");
        checked += 1;
    }

    if std::env::var("UPDATE_GOLDEN").is_err() {
        // Guards against the corpus directory quietly emptying and this test
        // passing by doing nothing.
        assert!(checked >= 5, "only {checked} corpus files were checked");
    }
}

// ---- title ---------------------------------------------------------------

#[test]
fn the_title_is_whitespace_collapsed_and_entities_decoded() {
    let r = parse_file("complete.html");
    assert_eq!(r.title.as_deref(), Some("Widgets & Gadgets"));
}

#[test]
fn an_empty_title_is_not_a_missing_title() {
    assert_eq!(parse_file("empty-title.html").title, Some(String::new()));
    assert_eq!(parse_file("minimal.html").title, None);
}

#[test]
fn only_the_first_title_counts() {
    let r = parse_str("<title>First</title><title>Second</title>");
    assert_eq!(r.title.as_deref(), Some("First"));
}

// ---- meta ----------------------------------------------------------------

#[test]
fn the_meta_description_is_read() {
    let r = parse_file("complete.html");
    assert_eq!(
        r.meta_description.as_deref(),
        Some("Everything about widgets.")
    );
}

#[test]
fn robots_and_googlebot_directives_are_merged_not_overwritten() {
    let r = parse_file("noindex.html");
    // robots says "noindex, all" and googlebot says "nofollow". Letting the
    // later tag win would report this page as indexable and followable.
    assert!(r.meta_robots.noindex);
    assert!(r.meta_robots.nofollow);
}

#[test]
fn a_page_level_nofollow_applies_to_every_link_on_it() {
    let r = parse_file("noindex.html");
    assert_eq!(r.links.len(), 1);
    assert!(
        r.links[0].nofollow,
        "the link carries no rel of its own, but the page says nofollow"
    );
}

// ---- open graph ----------------------------------------------------------

#[test]
fn open_graph_keeps_duplicates_in_document_order() {
    let r = parse_file("complete.html");
    assert_eq!(
        r.open_graph,
        [
            ("title".to_string(), "Widgets".to_string()),
            ("type".to_string(), "website".to_string()),
            ("title".to_string(), "Widgets (duplicate)".to_string()),
        ],
        "a duplicated og:title is a finding; a map would hide it"
    );
}

// ---- canonical and hreflang ---------------------------------------------

#[test]
fn the_canonical_is_kept_raw_and_resolved() {
    let r = parse_file("complete.html");
    assert_eq!(r.canonical.as_deref(), Some("/widgets"));
    assert_eq!(
        r.canonical_url.as_ref().map(|u| u.to_string()),
        Some("https://example.com/widgets".to_string())
    );
}

#[test]
fn rel_is_a_token_list_not_a_substring() {
    // `rel="not-canonical"` must not register as canonical.
    let r = parse_str(r#"<link rel="not-canonical" href="/wrong">"#);
    assert_eq!(r.canonical, None);

    let r = parse_str(r#"<link rel="canonical alternate" href="/right">"#);
    assert_eq!(r.canonical.as_deref(), Some("/right"));
}

#[test]
fn hreflang_values_are_lowercased_and_ordered() {
    let r = parse_file("complete.html");
    let got: Vec<(&str, &str)> = r
        .hreflang
        .iter()
        .map(|h| (h.lang.as_str(), h.href.as_str()))
        .collect();
    assert_eq!(got, [("en-gb", "/en-gb/widgets"), ("fr", "/fr/widgets")]);
}

// ---- headings ------------------------------------------------------------

#[test]
fn every_heading_is_captured_in_order() {
    let r = parse_file("complete.html");
    assert_eq!(r.h1, ["Widgets"]);
    assert_eq!(r.h2, ["Small widgets", "Large widgets"]);
}

#[test]
fn heading_text_spanning_child_elements_is_joined() {
    let r = parse_str("<h1>One <em>two</em> three</h1>");
    assert_eq!(r.h1, ["One two three"]);
}

// ---- links ---------------------------------------------------------------

#[test]
fn links_keep_the_href_as_written_and_resolve_it_against_the_page() {
    let r = parse_file("complete.html");
    let small = &r.links[0];
    assert_eq!(small.href, "/small");
    assert_eq!(
        small.target.as_ref().map(|u| u.to_string()),
        Some("https://example.com/small".to_string())
    );
    assert_eq!(small.text, "Small");
}

#[test]
fn a_relative_href_resolves_against_the_page_not_the_host_root() {
    let r = parse_str(r#"<a href="cart">Cart</a>"#);
    assert_eq!(
        r.links[0].target.as_ref().map(|u| u.to_string()),
        Some("https://example.com/shop/cart".to_string()),
        "the page is /shop/index.html, so a bare `cart` is /shop/cart"
    );
}

#[test]
fn a_nofollow_link_is_flagged_and_the_rest_are_not() {
    let r = parse_file("complete.html");
    let by_href = |h: &str| r.links.iter().find(|l| l.href == h).unwrap();
    assert!(by_href("/large").nofollow);
    assert!(!by_href("/small").nofollow);
}

#[test]
fn an_uncrawlable_href_is_recorded_with_no_target() {
    let r = parse_file("complete.html");
    let mail = r
        .links
        .iter()
        .find(|l| l.href.starts_with("mailto:"))
        .expect("a mailto link is still a link on the page");
    assert_eq!(mail.target, None);
}

#[test]
fn an_anchor_without_an_href_is_not_a_link() {
    let r = parse_file("complete.html");
    assert!(
        r.links.iter().all(|l| !l.text.contains("Not a link")),
        "<a name=...> is a jump target, not a link"
    );
}

#[test]
fn an_image_only_link_has_empty_text_rather_than_being_dropped() {
    let r = parse_file("complete.html");
    let link = r.links.iter().find(|l| l.href == "/img-only").unwrap();
    assert_eq!(
        link.text, "",
        "empty anchor text is an accessibility finding"
    );
}

// ---- images --------------------------------------------------------------

#[test]
fn a_missing_alt_and_an_empty_alt_are_different() {
    let r = parse_file("complete.html");
    let by_src = |s: &str| r.images.iter().find(|i| i.src == s).unwrap();
    assert_eq!(by_src("/hero.png").alt.as_deref(), Some("A hero"));
    assert_eq!(
        by_src("/spacer.gif").alt.as_deref(),
        Some(""),
        "a deliberate decorative marker"
    );
    assert_eq!(by_src("/bare.png").alt, None, "a defect");
}

// ---- word count ----------------------------------------------------------

#[test]
fn script_and_style_text_is_not_content() {
    let r = parse_file("complete.html");
    // "One two three four five." is the only sentence; the rest is headings,
    // link text, and code. The script's eight words must not be in here.
    assert!(
        r.word_count < 30,
        "word_count was {}, so script or style text leaked in",
        r.word_count
    );
    assert!(r.word_count >= 5);
}

#[test]
fn a_word_is_not_double_counted_when_it_spans_a_chunk_boundary() {
    let r = parse_str("<body><p>alpha beta gamma</p></body>");
    assert_eq!(r.word_count, 3);
}

#[test]
fn a_tag_boundary_ends_a_word() {
    // `<b>one</b><b>two</b>` renders as two words, not "onetwo".
    let r = parse_str("<body><b>one</b><b>two</b></body>");
    assert_eq!(r.word_count, 2);
}

#[test]
fn an_empty_body_has_no_words() {
    assert_eq!(parse_str("<body></body>").word_count, 0);
    assert_eq!(parse_str("<body>   \n  </body>").word_count, 0);
}

// ---- malformed markup ----------------------------------------------------

#[test]
fn broken_markup_still_yields_what_it_supports() {
    let r = parse_file("malformed.html");
    // Nothing in this file is closed properly, and it still has to report.
    assert_eq!(
        r.canonical.as_deref(),
        Some("/broken"),
        "unquoted attribute"
    );
    assert_eq!(
        r.meta_description.as_deref(),
        Some("unclosed everything"),
        "a meta tag inside an unclosed <title> is still a meta tag"
    );
    assert_eq!(r.links.len(), 2, "two unclosed anchors are two links");
    assert_eq!(r.images[0].alt.as_deref(), Some("unquoted"));
}

#[test]
fn extraction_never_reports_failure_on_ordinary_broken_html() {
    for name in ["complete.html", "minimal.html", "malformed.html"] {
        let html = std::fs::read(corpus_dir().join(name)).unwrap();
        let mut record = blank(BASE);
        assert!(extract(&mut record, &html).is_ok(), "{name}");
    }
}
