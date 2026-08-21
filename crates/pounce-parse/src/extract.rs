//! Single-pass extraction from HTML bytes into a `PageRecord`.
//!
//! One streaming pass with `lol_html`, building no DOM. That is not a
//! micro-optimisation: a crawler holds one page in memory per fetch worker, and
//! a tree per page is the difference between flat memory at 100k URLs and the
//! Electron competitor's profile. Everything here is written to survive that
//! constraint — the word counter never accumulates the text it counts, and no
//! handler keeps a buffer proportional to the page.
//!
//! Malformed markup is not an error condition. `lol_html` is a tolerant
//! streaming parser, so an unclosed tag or a stray `<` yields whatever the
//! markup supports and the record simply carries less. A crawler that refuses
//! to report on a broken page cannot audit the pages most in need of auditing.

use crate::body::{self, BodyKind};
use crate::record::{Hreflang, Image, Link, MetaRobots, PageRecord};
use lol_html::{HtmlRewriter, Settings, element, end_tag, text};
use std::cell::RefCell;
use std::rc::Rc;

/// Everything the handlers accumulate. Shared because each handler is a
/// separate closure and several of them write to the same fields.
#[derive(Default)]
struct State {
    title: Option<String>,
    /// Every `<title>` seen, not just the one kept. A second is a finding, and
    /// discarding it silently made that rule unwritable.
    title_count: u16,
    /// The first `<title>` has closed. A second one is a defect to report, not
    /// more of the first one's text.
    title_closed: bool,
    /// The anchor currently open had an `href`. Text inside an `<a>` with no
    /// href belongs to no link, and appending it to the previous one silently
    /// corrupts that link's anchor text.
    in_link: bool,
    meta_description: Option<String>,
    h1: Vec<String>,
    h2: Vec<String>,
    canonical: Option<String>,
    meta_robots: MetaRobots,
    hreflang: Vec<Hreflang>,
    open_graph: Vec<(String, String)>,
    links: Vec<Link>,
    images: Vec<Image>,
    words: Words,
    /// Nesting depth inside `<script>` or `<style>`, whose text is code rather
    /// than content and must not reach the word count.
    in_code: u32,
}

/// Counts and hashes body text without ever holding it.
///
/// `lol_html` delivers text in chunks that can split a word in half, so the
/// naive `split_whitespace()` per chunk both over-counts and — for the hash —
/// would make a page's value depend on where the parser happened to split it.
/// Buffering only the word currently being read fixes both: memory is one
/// word, and the hash is taken over exactly the whitespace-collapsed text the
/// count counts, so two pages differing only in markup agree.
#[derive(Default)]
struct Words {
    count: u32,
    /// The word being read. Bounded by word length, never by page size.
    word: String,
    hash: crate::hash::Fnv1a,
    /// Whether any word has been hashed, so separators go *between* words and
    /// an empty body stays distinguishable from a hash of nothing.
    any: bool,
}

impl Words {
    fn feed(&mut self, chunk: &str) {
        for c in chunk.chars() {
            if c.is_whitespace() {
                self.finish();
            } else {
                self.word.push(c);
            }
        }
    }

    fn finish(&mut self) {
        if self.word.is_empty() {
            return;
        }
        if self.any {
            self.hash.write(" ");
        }
        self.hash.write(&self.word);
        self.word.clear();
        self.count = self.count.saturating_add(1);
        self.any = true;
    }

    /// `None` when the body had no text at all — different from the hash of
    /// the empty string, which would make every blank page a duplicate.
    fn body_hash(&self) -> Option<u64> {
        self.any.then(|| self.hash.finish())
    }
}

/// Collapses runs of whitespace and trims, the way a browser renders text.
///
/// Anchor text spanning several source lines must compare equal to the same
/// text on one line, or every audit rule about link text becomes markup-shape
/// dependent.
fn collapse(s: &str) -> String {
    // Decoded before collapsing, so `&nbsp;` becomes a space that then folds
    // into the run beside it, the way a browser renders it.
    let decoded = html_escape::decode_html_entities(s);
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The entry point a crawl uses: classify the body, then extract only if it is
/// HTML.
///
/// Kept separate from [`extract`] so the extractor stays a pure
/// markup-in/record-out function that tests can call directly, while the
/// decision about *whether* to run it lives in one place rather than at every
/// call site.
pub fn parse_body(record: &mut PageRecord, body: &[u8]) -> Result<(), String> {
    record.kind = body::classify(record.content_type.as_deref());
    record.content_type_mismatch = body::is_mismatch(record.kind, body);

    // Declared HTML that is demonstrably not HTML is skipped rather than
    // parsed: the mismatch is already recorded, and running the extractor over
    // a PDF spends a pass to produce nothing.
    if record.kind == BodyKind::Html && !record.content_type_mismatch {
        return extract(record, body);
    }
    Ok(())
}

/// Fills the extracted half of `record` from `html`.
///
/// Never fails on markup: a body that cannot be parsed at all leaves the
/// extracted fields empty and the transport facts intact. The `Err` case is
/// reserved for `lol_html` refusing the input outright, which the caller
/// should record rather than retry.
pub fn extract(record: &mut PageRecord, html: &[u8]) -> Result<(), String> {
    let state = Rc::new(RefCell::new(State::default()));

    let settings = Settings::new()
        // ---- <title> ---------------------------------------------------
        .append_element_content_handler(element!("title", {
            let state = Rc::clone(&state);
            move |el| {
                // Present-but-empty is set here so that `<title></title>`
                // becomes Some("") rather than None. The two are different
                // findings and the record must not merge them.
                let mut s = state.borrow_mut();
                s.title_count = s.title_count.saturating_add(1);
                if s.title.is_none() {
                    s.title = Some(String::new());
                }
                drop(s);
                let state = Rc::clone(&state);
                if el.is_self_closing() {
                    state.borrow_mut().title_closed = true;
                } else {
                    let _ = el.on_end_tag(end_tag!(move |_| {
                        state.borrow_mut().title_closed = true;
                        Ok(())
                    }));
                }
                Ok(())
            }
        }))
        .append_element_content_handler(text!("title", {
            let state = Rc::clone(&state);
            move |t| {
                let mut s = state.borrow_mut();
                if s.title_closed {
                    return Ok(());
                }
                if let Some(title) = s.title.as_mut() {
                    title.push_str(t.as_str());
                }
                Ok(())
            }
        }))
        // ---- <meta> ----------------------------------------------------
        // One handler rather than a selector per case: `name` and `property`
        // values are case-insensitive in practice, and CSS attribute
        // selectors are not, so matching in code is both shorter and correct.
        .append_element_content_handler(element!("meta", {
            let state = Rc::clone(&state);
            move |el| {
                let content = el.get_attribute("content").unwrap_or_default();
                let name = el.get_attribute("name").unwrap_or_default().to_lowercase();
                let property = el
                    .get_attribute("property")
                    .unwrap_or_default()
                    .to_lowercase();
                let mut s = state.borrow_mut();

                match name.as_str() {
                    "description" if s.meta_description.is_none() => {
                        s.meta_description = Some(content.clone());
                    }
                    // Both are honoured, and merged rather than overwritten:
                    // a page saying `robots: noindex` and `googlebot: all`
                    // is still noindex.
                    "robots" | "googlebot" => {
                        s.meta_robots = s.meta_robots.or(MetaRobots::parse(&content));
                    }
                    _ => {}
                }

                if let Some(og) = property.strip_prefix("og:") {
                    s.open_graph.push((og.to_string(), content));
                }
                Ok(())
            }
        }))
        // ---- <link> ----------------------------------------------------
        .append_element_content_handler(element!("link", {
            let state = Rc::clone(&state);
            move |el| {
                let rel = el.get_attribute("rel").unwrap_or_default().to_lowercase();
                let href = el.get_attribute("href").unwrap_or_default();
                // `rel` is a space-separated token list; `rel="canonical x"`
                // is valid and a substring match would also fire on
                // `rel="not-canonical"`.
                let rels: Vec<&str> = rel.split_whitespace().collect();
                let mut s = state.borrow_mut();

                if rels.contains(&"canonical") && s.canonical.is_none() {
                    s.canonical = Some(href.clone());
                }
                if rels.contains(&"alternate")
                    && let Some(lang) = el.get_attribute("hreflang")
                {
                    s.hreflang.push(Hreflang {
                        lang: lang.to_lowercase(),
                        href,
                    });
                }
                Ok(())
            }
        }))
        // ---- headings --------------------------------------------------
        .append_element_content_handler(element!("h1", {
            let state = Rc::clone(&state);
            move |_el| {
                state.borrow_mut().h1.push(String::new());
                Ok(())
            }
        }))
        .append_element_content_handler(text!("h1", {
            let state = Rc::clone(&state);
            move |t| {
                let mut s = state.borrow_mut();
                if let Some(last) = s.h1.last_mut() {
                    last.push_str(t.as_str());
                }
                Ok(())
            }
        }))
        .append_element_content_handler(element!("h2", {
            let state = Rc::clone(&state);
            move |_el| {
                state.borrow_mut().h2.push(String::new());
                Ok(())
            }
        }))
        .append_element_content_handler(text!("h2", {
            let state = Rc::clone(&state);
            move |t| {
                let mut s = state.borrow_mut();
                if let Some(last) = s.h2.last_mut() {
                    last.push_str(t.as_str());
                }
                Ok(())
            }
        }))
        // ---- anchors ---------------------------------------------------
        .append_element_content_handler(element!("a", {
            let state = Rc::clone(&state);
            move |el| {
                let href = el.get_attribute("href");
                // An `<a>` with no href is a jump target, not a link — and its
                // text must not land on the previous link either.
                state.borrow_mut().in_link = href.is_some();
                if let Some(href) = href {
                    let rel = el.get_attribute("rel").unwrap_or_default().to_lowercase();
                    state.borrow_mut().links.push(Link {
                        href,
                        target: None,
                        text: String::new(),
                        nofollow: rel.split_whitespace().any(|t| t == "nofollow"),
                    });
                }
                Ok(())
            }
        }))
        .append_element_content_handler(text!("a", {
            let state = Rc::clone(&state);
            move |t| {
                let mut s = state.borrow_mut();
                if !s.in_link {
                    return Ok(());
                }
                if let Some(last) = s.links.last_mut() {
                    last.text.push_str(t.as_str());
                }
                Ok(())
            }
        }))
        // ---- images ----------------------------------------------------
        .append_element_content_handler(element!("img", {
            let state = Rc::clone(&state);
            move |el| {
                state.borrow_mut().images.push(Image {
                    src: el.get_attribute("src").unwrap_or_default(),
                    // Absent and empty are kept apart: an empty alt is a
                    // deliberate decorative marker, a missing one is a defect.
                    alt: el.get_attribute("alt"),
                });
                Ok(())
            }
        }))
        // ---- word count ------------------------------------------------
        .append_element_content_handler(element!("script, style", {
            let state = Rc::clone(&state);
            move |el| {
                state.borrow_mut().in_code += 1;
                let state = Rc::clone(&state);
                // Self-closing and void forms never produce an end tag, so
                // the counter would never come back down.
                if !el.is_self_closing() {
                    let _ = el.on_end_tag(end_tag!(move |_| {
                        let mut s = state.borrow_mut();
                        s.in_code = s.in_code.saturating_sub(1);
                        Ok(())
                    }));
                }
                Ok(())
            }
        }))
        .append_element_content_handler(text!("body", {
            let state = Rc::clone(&state);
            move |t| {
                let mut s = state.borrow_mut();
                if s.in_code == 0 {
                    s.words.feed(t.as_str());
                }
                // A word ends at a tag boundary as surely as at a space:
                // `<b>one</b><b>two</b>` is two words, not one.
                if t.last_in_text_node() {
                    s.words.finish();
                }
                Ok(())
            }
        }));

    let mut rewriter = HtmlRewriter::new(settings, |_: &[u8]| {});
    let outcome = rewriter
        .write(html)
        .and_then(|()| rewriter.end())
        .map_err(|e| e.to_string());
    // A mid-document failure still leaves whatever was extracted before it,
    // which is more useful in a report than an empty record.
    let mut state = Rc::try_unwrap(state)
        .map(RefCell::into_inner)
        .unwrap_or_default();
    state.words.finish();

    record.title_count = state.title_count;
    record.body_hash = state.words.body_hash();
    record.title = state.title.map(|t| collapse(&t));
    record.meta_description = state.meta_description.map(|d| collapse(&d));
    record.h1 = state.h1.iter().map(|h| collapse(h)).collect();
    record.h2 = state.h2.iter().map(|h| collapse(h)).collect();
    record.canonical_url = state
        .canonical
        .as_deref()
        .and_then(|href| record.url.join(href).ok());
    record.canonical = state.canonical;
    record.meta_robots = state.meta_robots;
    record.hreflang = state.hreflang;
    record.open_graph = state.open_graph;
    record.images = state.images;
    record.word_count = state.words.count;
    record.links = state
        .links
        .into_iter()
        .map(|mut link| {
            link.target = record.url.join(&link.href).ok();
            link.text = collapse(&link.text);
            // A page-level `nofollow` applies to every link on it.
            link.nofollow |= record.meta_robots.nofollow;
            link
        })
        .collect();

    outcome
}
