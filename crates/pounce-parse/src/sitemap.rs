//! XML sitemaps: the `<loc>` values, and whether the document is an index.
//!
//! Hand-written rather than a dependency, and the reason is scale rather than
//! taste. A sitemap is one element repeated — `<url><loc>…</loc></url>` — and
//! the crawler needs exactly two facts from it: the addresses, and whether
//! they are pages or further sitemaps. A general XML parser reads a document
//! model this never asks a question of.
//!
//! What it *does* have to get right is entity decoding. A sitemap URL with a
//! query string is written `?a=1&amp;b=2`, and treating that literally
//! produces a URL that 404s — a "not crawled" finding invented by the reader.
//! `html-escape` decodes the five XML entities and the numeric forms, and it
//! is already a dependency for the same reason on the HTML side.

/// What a sitemap document turned out to be.
///
/// Told apart by the root element, not by the filename: `sitemap_index.xml`
/// is a convention and nothing more, and a crawler that trusts it follows a
/// list of pages as though they were sitemaps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sitemap {
    /// Every `<loc>` in document order, entity-decoded, duplicates kept — a
    /// sitemap listing the same URL twice is itself worth reporting.
    pub locations: Vec<String>,
    /// True when the root is `<sitemapindex>`: these locations are sitemaps.
    pub is_index: bool,
}

/// The most `<loc>` values one document may contribute.
///
/// The sitemap protocol caps a file at 50,000 URLs. This is the same number,
/// applied to a file that ignores its own spec: without it, a malformed or
/// hostile sitemap decides how much memory the crawler uses.
pub const MAX_LOCATIONS: usize = 50_000;

/// Reads a sitemap or sitemap index.
///
/// Never fails. A document that is not a sitemap yields no locations, which is
/// the same answer as an empty sitemap and needs the same handling — the
/// caller reports "no URLs found here" either way.
pub fn parse(xml: &str) -> Sitemap {
    // The root element decides. Searched rather than parsed to it, because a
    // sitemap may open with a comment, a stylesheet instruction, or a BOM, and
    // the first `<` is not reliably the root.
    let is_index = xml.contains("<sitemapindex") || xml.contains(":sitemapindex");

    let mut locations = Vec::new();
    let mut rest = xml;
    while let Some(open) = find_loc_open(rest) {
        rest = &rest[open..];
        // Past the `>` of the opening tag, which may carry attributes.
        let Some(gt) = rest.find('>') else { break };
        rest = &rest[gt + 1..];
        let Some(close) = rest.find("</") else { break };
        let raw = rest[..close].trim();
        if !raw.is_empty() {
            locations.push(html_escape::decode_html_entities(raw).into_owned());
        }
        rest = &rest[close..];
        if locations.len() >= MAX_LOCATIONS {
            break;
        }
    }

    Sitemap {
        locations,
        is_index,
    }
}

/// The next `<loc>` opening tag that names a page.
///
/// A prefix is allowed — `<sm:loc>` is the sitemap namespace spelled
/// explicitly — but three prefixes are not. The image, video and news
/// extensions each nest their own `<loc>` inside a `<url>`, and those are
/// media addresses. Counting them as pages reports images as URLs missing
/// from the crawl, which is a finding about the reader rather than the site.
fn find_loc_open(haystack: &str) -> Option<usize> {
    const NOT_PAGES: [&str; 3] = ["image", "video", "news"];
    let bytes = haystack.as_bytes();
    let mut from = 0;
    while let Some(at) = haystack[from..].find("loc") {
        let start = from + at;
        from = start + 3;

        // What follows has to end the element name.
        if !matches!(
            bytes.get(start + 3),
            Some(b'>') | Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r')
        ) {
            continue;
        }

        // What precedes has to open one: `<loc` directly, or `<prefix:loc`.
        match start.checked_sub(1).map(|i| bytes[i]) {
            Some(b'<') => return Some(start),
            Some(b':') => {
                let head = &haystack[..start - 1];
                let Some(open) = head.rfind('<') else {
                    continue;
                };
                let prefix = &head[open + 1..];
                if !NOT_PAGES.contains(&prefix) {
                    return Some(start - 1 - prefix.len() - 1);
                }
            }
            _ => continue,
        }
    }
    None
}
