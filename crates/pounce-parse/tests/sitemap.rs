//! Reading XML sitemaps.

use pounce_parse::sitemap::{MAX_LOCATIONS, parse};

const URLSET: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url><loc>https://example.com/</loc><lastmod>2026-01-01</lastmod></url>
  <url><loc>https://example.com/about</loc></url>
</urlset>"#;

#[test]
fn a_urlset_gives_its_locations_in_order() {
    let map = parse(URLSET);
    assert!(!map.is_index);
    assert_eq!(
        map.locations,
        vec!["https://example.com/", "https://example.com/about"]
    );
}

#[test]
fn an_index_is_told_apart_by_its_root_not_its_filename() {
    // `sitemap_index.xml` is a convention. A crawler that trusts the filename
    // follows a list of pages as though each were a sitemap.
    let xml = r#"<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
        <sitemap><loc>https://example.com/sitemap-1.xml</loc></sitemap>
    </sitemapindex>"#;
    let map = parse(xml);
    assert!(map.is_index);
    assert_eq!(map.locations, vec!["https://example.com/sitemap-1.xml"]);
}

#[test]
fn entities_are_decoded_or_the_url_is_a_different_url() {
    // `&amp;` is how a query string is spelled in XML. Left literal, the URL
    // 404s and the crawl reports a sitemap entry it could not reach — a
    // finding invented by the reader rather than found on the site.
    let xml = "<urlset><url><loc>https://example.com/s?a=1&amp;b=2</loc></url></urlset>";
    assert_eq!(parse(xml).locations, vec!["https://example.com/s?a=1&b=2"]);
}

#[test]
fn a_namespace_prefix_is_still_a_loc() {
    let xml = "<sm:urlset><sm:url><sm:loc>https://example.com/x</sm:loc></sm:url></sm:urlset>";
    assert_eq!(parse(xml).locations, vec!["https://example.com/x"]);
}

#[test]
fn an_image_loc_is_not_a_page() {
    // Image sitemap extensions nest `<image:loc>` inside a `<url>`. Counting
    // those as pages would report images as URLs missing from the crawl.
    let xml = r#"<urlset><url>
        <loc>https://example.com/p</loc>
        <image:image><image:loc>https://example.com/a.jpg</image:loc></image:image>
    </url></urlset>"#;
    assert_eq!(parse(xml).locations, vec!["https://example.com/p"]);
}

#[test]
fn a_document_that_is_not_a_sitemap_yields_nothing_rather_than_failing() {
    // A server answering the sitemap URL with an HTML error page is ordinary.
    for xml in [
        "<html><body>Not found</body></html>",
        "",
        "{\"json\": true}",
    ] {
        let map = parse(xml);
        assert!(map.locations.is_empty(), "{xml} produced locations");
        assert!(!map.is_index);
    }
}

#[test]
fn a_location_element_is_not_a_loc() {
    let xml = "<urlset><location>https://example.com/no</location></urlset>";
    assert!(parse(xml).locations.is_empty());
}

#[test]
fn the_cap_bounds_what_one_document_can_cost() {
    // The protocol caps a file at 50,000 URLs. Without applying it here, a
    // malformed or hostile sitemap decides how much memory the crawler uses.
    let body = "<url><loc>https://example.com/x</loc></url>".repeat(MAX_LOCATIONS + 100);
    assert_eq!(
        parse(&format!("<urlset>{body}</urlset>")).locations.len(),
        MAX_LOCATIONS
    );
}

#[test]
fn whitespace_around_a_location_is_not_part_of_it() {
    let xml = "<urlset><url><loc>\n    https://example.com/p\n  </loc></url></urlset>";
    assert_eq!(parse(xml).locations, vec!["https://example.com/p"]);
}

#[test]
fn the_cap_says_when_it_cut_something_off() {
    // A silent cap under-reports the site's own sitemap and turns every URL
    // past it into a page "listed nowhere" — a finding invented by our bound.
    let one = "<url><loc>https://example.com/x</loc></url>";
    let over = parse(&format!(
        "<urlset>{}</urlset>",
        one.repeat(MAX_LOCATIONS + 1)
    ));
    assert_eq!(over.locations.len(), MAX_LOCATIONS);
    assert!(over.truncated, "the cap cut URLs off and did not say so");

    // Exactly at the cap is complete, not truncated.
    let exact = parse(&format!("<urlset>{}</urlset>", one.repeat(MAX_LOCATIONS)));
    assert_eq!(exact.locations.len(), MAX_LOCATIONS);
    assert!(!exact.truncated, "a full document is not a cut one");

    let small = parse(&format!("<urlset>{one}</urlset>"));
    assert!(!small.truncated);
}
