//! What a response body *is*, and whether the HTML extractor should see it.
//!
//! A crawl hits PDFs, images, JSON APIs and zip files, and every one of them is
//! a URL the report has to account for. Running the HTML extractor over them
//! wastes the pass and produces a record full of empty fields that reads like a
//! page with no title rather than a file that never had one.
//!
//! The declared `Content-Type` decides. Magic bytes are used for exactly one
//! purpose — noticing that the declaration is *wrong* — and never to override
//! it. That split is deliberate: a mismatch is a finding the user can act on,
//! whereas silently re-classifying would put a guess into a crawl report and
//! leave the server's actual misconfiguration invisible.

use serde::{Deserialize, Serialize};

/// What a body turned out to be, as far as the crawler needs to care.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyKind {
    /// Goes to the HTML extractor.
    Html,
    Pdf,
    Image,
    /// A type we recognise and deliberately do not parse — JSON, CSS, zip.
    Other,
    /// No `Content-Type` at all. Distinct from `Other`, because "the server
    /// declared nothing" and "the server declared something we skip" are
    /// different server-side problems.
    #[default]
    Undeclared,
}

/// Classifies by the declared media type alone.
pub fn classify(mime: Option<&str>) -> BodyKind {
    let Some(mime) = mime else {
        return BodyKind::Undeclared;
    };
    match mime {
        "text/html" | "application/xhtml+xml" => BodyKind::Html,
        "application/pdf" => BodyKind::Pdf,
        _ if mime.starts_with("image/") => BodyKind::Image,
        _ => BodyKind::Other,
    }
}

/// The kind implied by the body's leading bytes, when they are unambiguous.
///
/// Only signatures with no realistic false positives are listed. This exists to
/// contradict a declaration, so a shaky guess here would manufacture findings.
pub fn sniff(body: &[u8]) -> Option<BodyKind> {
    if body.starts_with(b"%PDF-") {
        return Some(BodyKind::Pdf);
    }
    if body.starts_with(b"\x89PNG\r\n\x1a\n")
        || body.starts_with(b"\xff\xd8\xff")
        || body.starts_with(b"GIF87a")
        || body.starts_with(b"GIF89a")
        || (body.len() >= 12 && body.starts_with(b"RIFF") && &body[8..12] == b"WEBP")
    {
        return Some(BodyKind::Image);
    }
    // Markup is the one case with no fixed signature. Leading whitespace is
    // skipped because servers emit it and a browser ignores it; the window is
    // bounded so a megabyte of spaces cannot turn this into a scan.
    let head = &body[..body.len().min(512)];
    let start = head
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(head.len());
    let head = &head[start..];
    let looks_like =
        |tag: &[u8]| head.len() >= tag.len() && head[..tag.len()].eq_ignore_ascii_case(tag);
    if looks_like(b"<!doctype html") || looks_like(b"<html") {
        return Some(BodyKind::Html);
    }
    None
}

/// True when the server declared one kind and sent another.
pub fn is_mismatch(declared: BodyKind, body: &[u8]) -> bool {
    // Nothing to contradict, and reporting one anyway would be the guess this
    // module exists to avoid.
    if declared == BodyKind::Undeclared {
        return false;
    }
    sniff(body).is_some_and(|actual| actual != declared)
}
