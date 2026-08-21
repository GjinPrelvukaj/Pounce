//! Non-HTML responses: what gets parsed, what gets recorded, what gets flagged.

use pounce_core::CrawlUrl;
use pounce_parse::body::{BodyKind, classify, is_mismatch, sniff};
use pounce_parse::{MetaRobots, PageRecord, parse_body};

const PDF: &[u8] = b"%PDF-1.7\n1 0 obj\n";
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR";
const JPEG: &[u8] = b"\xff\xd8\xff\xe0\x00\x10JFIF";
const GIF: &[u8] = b"GIF89a\x01\x00";
const HTML: &[u8] = b"<!DOCTYPE html><html><head><title>Real</title></head><body>a b</body></html>";

fn record(mime: Option<&str>) -> PageRecord {
    PageRecord {
        url: CrawlUrl::parse("https://example.com/file").unwrap(),
        status: 200,
        depth: 0,
        size: 0,
        truncated: false,
        content_type: mime.map(str::to_string),
        charset: None,
        kind: BodyKind::Undeclared,
        content_type_mismatch: false,
        elapsed_ms: 0,
        time_to_headers_ms: 0,
        redirect_chain: vec![],
        title: None,
        title_count: 0,
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
        body_hash: None,
    }
}

// ---- classification by declaration --------------------------------------

#[test]
fn html_types_are_the_only_ones_that_parse() {
    assert_eq!(classify(Some("text/html")), BodyKind::Html);
    assert_eq!(classify(Some("application/xhtml+xml")), BodyKind::Html);
}

#[test]
fn the_common_binary_types_are_named_rather_than_lumped_together() {
    assert_eq!(classify(Some("application/pdf")), BodyKind::Pdf);
    assert_eq!(classify(Some("image/png")), BodyKind::Image);
    assert_eq!(classify(Some("image/svg+xml")), BodyKind::Image);
    assert_eq!(classify(Some("application/json")), BodyKind::Other);
    assert_eq!(classify(Some("text/css")), BodyKind::Other);
}

#[test]
fn an_undeclared_type_is_not_the_same_as_a_skipped_one() {
    // "the server declared nothing" and "the server declared something we do
    // not parse" are different server-side problems.
    assert_eq!(classify(None), BodyKind::Undeclared);
    assert_ne!(classify(None), classify(Some("application/zip")));
}

// ---- sniffing exists only to contradict ---------------------------------

#[test]
fn sniffing_recognises_only_unambiguous_signatures() {
    assert_eq!(sniff(PDF), Some(BodyKind::Pdf));
    assert_eq!(sniff(PNG), Some(BodyKind::Image));
    assert_eq!(sniff(JPEG), Some(BodyKind::Image));
    assert_eq!(sniff(GIF), Some(BodyKind::Image));
    assert_eq!(sniff(HTML), Some(BodyKind::Html));
}

#[test]
fn sniffing_declines_on_anything_it_cannot_be_sure_of() {
    // A shaky guess here would manufacture findings, so plain text, JSON, and
    // a short body must all come back as "no opinion".
    assert_eq!(sniff(b"just some words"), None);
    assert_eq!(sniff(b"{\"a\":1}"), None);
    assert_eq!(sniff(b""), None);
    assert_eq!(sniff(b"%P"), None, "a truncated signature is not a match");
}

#[test]
fn leading_whitespace_does_not_hide_html() {
    assert_eq!(sniff(b"\n\n   <html><body>x"), Some(BodyKind::Html));
}

// ---- mismatch ------------------------------------------------------------

#[test]
fn a_pdf_served_as_html_is_a_mismatch() {
    assert!(is_mismatch(BodyKind::Html, PDF));
}

#[test]
fn a_body_matching_its_declaration_is_not_a_mismatch() {
    assert!(!is_mismatch(BodyKind::Html, HTML));
    assert!(!is_mismatch(BodyKind::Pdf, PDF));
    assert!(!is_mismatch(BodyKind::Image, PNG));
}

#[test]
fn no_signature_means_no_mismatch() {
    // Absence of evidence is not a finding.
    assert!(!is_mismatch(BodyKind::Other, b"arbitrary bytes"));
    assert!(!is_mismatch(
        BodyKind::Html,
        b"not really markup but who knows"
    ));
}

#[test]
fn an_undeclared_type_is_never_a_mismatch() {
    // There is nothing to contradict, and reporting one would be a way of
    // sneaking the guess back in.
    assert!(!is_mismatch(BodyKind::Undeclared, PDF));
}

// ---- the entry point -----------------------------------------------------

#[test]
fn html_is_extracted() {
    let mut r = record(Some("text/html"));
    parse_body(&mut r, HTML).unwrap();
    assert_eq!(r.kind, BodyKind::Html);
    assert_eq!(r.title.as_deref(), Some("Real"));
    assert_eq!(r.word_count, 2);
}

#[test]
fn a_pdf_is_recorded_but_never_parsed_as_markup() {
    let mut r = record(Some("application/pdf"));
    parse_body(&mut r, PDF).unwrap();

    assert_eq!(r.kind, BodyKind::Pdf);
    assert!(!r.content_type_mismatch);
    // Not "a page with no title" — a file that never had one.
    assert_eq!(r.title, None);
    assert_eq!(r.word_count, 0);
    assert!(r.links.is_empty());
}

#[test]
fn a_pdf_declared_as_html_is_flagged_and_still_not_parsed_as_html() {
    let mut r = record(Some("text/html"));
    parse_body(&mut r, PDF).unwrap();

    assert!(
        r.content_type_mismatch,
        "the finding is the misconfiguration"
    );
    // The declaration still decides the kind. Trusting the bytes instead would
    // hide the server's mistake, which is the thing worth reporting.
    assert_eq!(r.kind, BodyKind::Html);
    // But running the markup extractor over a PDF is pure waste, so it did not
    // happen: no title, no words, no links.
    assert_eq!(r.title, None);
    assert_eq!(r.word_count, 0);
}

#[test]
fn an_undeclared_body_is_not_parsed_and_not_guessed_at() {
    let mut r = record(None);
    parse_body(&mut r, HTML).unwrap();

    assert_eq!(r.kind, BodyKind::Undeclared);
    assert!(!r.content_type_mismatch);
    assert_eq!(
        r.title, None,
        "a guess in a crawl report is worse than none"
    );
}

#[test]
fn a_truncated_html_body_is_still_extracted_from() {
    // The cap protects memory; it must not silently blank the record. What was
    // read is still a partial page, and `truncated` already says so.
    let mut r = record(Some("text/html"));
    r.truncated = true;
    let cut = &HTML[..HTML.len() - 20];
    parse_body(&mut r, cut).unwrap();

    assert_eq!(r.title.as_deref(), Some("Real"));
    assert!(r.truncated);
}
