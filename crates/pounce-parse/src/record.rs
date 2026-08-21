//! `PageRecord` — the shared vocabulary.
//!
//! Everything downstream of the fetch speaks this type: the store writes it,
//! the audit rules read it, the exporters serialise it. Defining it before the
//! extractors exist is deliberate — a record shape that grows a field per
//! extractor ends up describing the parser rather than the page, and every
//! audit rule then has to know which parser produced it.
//!
//! Two rules hold throughout:
//!
//! **Raw and resolved are both kept.** A `<link rel="canonical" href="/x">` is
//! reported to the user as `/x` — what the page actually says — and compared
//! against `https://host/x` when auditing. Storing only the resolved form
//! makes a remediation message quote a URL that appears nowhere in the source.
//!
//! **Absent and empty are different.** `Option::None` means the page had no
//! such element; `Some("")` means it had an empty one. `<title></title>` is a
//! findable SEO defect and a missing `<title>` is a different one, so the type
//! must not merge them.

use crate::body::BodyKind;
use pounce_core::CrawlUrl;
use pounce_http::fetch::Fetched;
use serde::{Deserialize, Serialize};

/// One crawled page, as far as the parser is concerned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageRecord {
    // ---- what the fetch established -----------------------------------
    pub url: CrawlUrl,
    pub status: u16,
    /// Hops from the seed. `0` is the seed itself.
    pub depth: u16,
    /// Bytes actually read, after any truncation.
    pub size: usize,
    /// True when the body was longer than the configured ceiling, so every
    /// extracted field below may be incomplete.
    pub truncated: bool,
    pub content_type: Option<String>,
    pub charset: Option<String>,
    /// What the body turned out to be. Only `Html` reaches the extractor, so
    /// this is also the reason a record has no title.
    pub kind: BodyKind,
    /// The declared type and the body's leading bytes disagree — a PDF served
    /// as `text/html`, say. Recorded, never acted on: silently trusting the
    /// bytes would hide the server's misconfiguration, which is the finding.
    pub content_type_mismatch: bool,
    pub elapsed_ms: u32,
    pub time_to_headers_ms: u32,
    /// URLs crossed to reach this page, in order, empty for a direct hit. The
    /// chain is a finding in its own right, not routing detail.
    pub redirect_chain: Vec<String>,

    // ---- what the markup said -----------------------------------------
    pub title: Option<String>,
    /// Every `<title>` the document contained, not just the one kept in
    /// `title`. A second one is a finding; the extractor keeps the first.
    pub title_count: u16,
    pub meta_description: Option<String>,
    pub h1: Vec<String>,
    pub h2: Vec<String>,
    /// The `href` of `<link rel="canonical">`, exactly as written.
    pub canonical: Option<String>,
    /// `canonical` resolved against `url`; `None` when absent or unparseable.
    pub canonical_url: Option<CrawlUrl>,
    pub meta_robots: MetaRobots,
    pub hreflang: Vec<Hreflang>,
    /// `og:*` properties in document order, names with the `og:` prefix
    /// stripped. A list rather than a map because duplicates are themselves a
    /// finding, and a map would silently keep only the last one.
    pub open_graph: Vec<(String, String)>,
    pub links: Vec<Link>,
    pub images: Vec<Image>,
    /// Words in the rendered text, excluding markup, script, and style.
    pub word_count: u32,
    /// FNV-1a of the same whitespace-collapsed text `word_count` counts.
    ///
    /// `None` when the page had no text at all — different from the hash of
    /// the empty string, which would make every blank page a duplicate of
    /// every other.
    pub body_hash: Option<u64>,
}

impl PageRecord {
    /// Seeds a record from the fetch, with every extracted field still empty.
    ///
    /// Splitting it this way keeps the two failure modes apart: everything set
    /// here is a fact the transport established and cannot be wrong about,
    /// while everything below it is a reading of untrusted markup. A record
    /// whose extraction failed still carries a correct status, size, and
    /// timing, which is what the report needs to say *why* a page is blank.
    pub fn from_fetched(fetched: &Fetched, depth: u16, redirect_chain: Vec<String>) -> Self {
        Self {
            url: fetched.url.clone(),
            status: fetched.status.as_u16(),
            depth,
            size: fetched.size(),
            truncated: fetched.truncated,
            content_type: fetched.mime(),
            charset: fetched.charset(),
            kind: BodyKind::Undeclared,
            content_type_mismatch: false,
            // Saturating rather than wrapping: a crawl that ran past 49 days
            // on one request should report a preposterous number, not a small
            // one. u32 milliseconds is ~49 days, so this never fires in
            // practice and exists so that it cannot lie if it does.
            elapsed_ms: fetched.elapsed.as_millis().min(u32::MAX as u128) as u32,
            time_to_headers_ms: fetched.time_to_headers.as_millis().min(u32::MAX as u128) as u32,
            redirect_chain,

            title: None,
            title_count: 0,
            meta_description: None,
            h1: Vec::new(),
            h2: Vec::new(),
            canonical: None,
            canonical_url: None,
            meta_robots: MetaRobots::default(),
            hreflang: Vec::new(),
            open_graph: Vec::new(),
            links: Vec::new(),
            images: Vec::new(),
            word_count: 0,
            body_hash: None,
        }
    }
}

/// One `<a href>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    /// The `href` exactly as written, relative form included.
    pub href: String,
    /// `href` resolved against the page URL; `None` when it is not crawlable
    /// — a `mailto:`, a `javascript:`, or simply malformed.
    pub target: Option<CrawlUrl>,
    /// Anchor text, whitespace-collapsed. Empty for an image-only link, which
    /// is an accessibility finding rather than a parse failure.
    pub text: String,
    /// `rel` contained `nofollow`, on the link or via a page-level directive.
    pub nofollow: bool,
}

/// One `<img>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Image {
    pub src: String,
    /// `None` when the attribute is absent, `Some("")` when it is present and
    /// empty — the second is a deliberate decorative marker, the first is a
    /// defect, and an audit rule must be able to tell them apart.
    pub alt: Option<String>,
}

/// One `<link rel="alternate" hreflang="...">`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hreflang {
    pub lang: String,
    pub href: String,
}

/// The indexing directives a page declares about itself.
///
/// Defaults to fully indexable, which is what a page with no `<meta
/// name="robots">` means. Note this is the *page's* claim; robots.txt is a
/// separate, earlier decision about whether to fetch at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MetaRobots {
    pub noindex: bool,
    pub nofollow: bool,
    pub noarchive: bool,
    pub nosnippet: bool,
}

impl MetaRobots {
    /// Parses one `content` value: `noindex, nofollow`, `none`, `all`.
    pub fn parse(content: &str) -> Self {
        let mut out = Self::default();
        for token in content.split(',') {
            match token.trim().to_ascii_lowercase().as_str() {
                // `none` is defined as noindex + nofollow together.
                "none" => {
                    out.noindex = true;
                    out.nofollow = true;
                }
                "noindex" => out.noindex = true,
                "nofollow" => out.nofollow = true,
                "noarchive" => out.noarchive = true,
                "nosnippet" => out.nosnippet = true,
                // `all` and anything unrecognised leave the defaults alone.
                // Directives are additive: a later `all` does not cancel an
                // earlier `noindex`, and neither does an unknown token.
                _ => {}
            }
        }
        out
    }

    /// Merges a second directive source, keeping every restriction.
    ///
    /// A page may carry both `<meta name="robots">` and `<meta
    /// name="googlebot">`, and an `X-Robots-Tag` header on top. The union is
    /// the safe reading: no source can loosen what another tightened.
    pub fn or(self, other: Self) -> Self {
        Self {
            noindex: self.noindex || other.noindex,
            nofollow: self.nofollow || other.nofollow,
            noarchive: self.noarchive || other.noarchive,
            nosnippet: self.nosnippet || other.nosnippet,
        }
    }

    pub fn is_indexable(self) -> bool {
        !self.noindex
    }
}
