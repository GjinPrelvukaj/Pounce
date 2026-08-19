//! Renders a [`PageNode`](crate::graph::PageNode) into an HTML document.
//!
//! The markup is deliberately ordinary. The goal is a page that exercises a
//! crawler the way a real CMS would — nav, body copy, images, a long list of
//! outlinks — not markup that shows off.

use crate::graph::SiteGraph;
use std::fmt::Write as _;

/// Body filler, repeated to reach the node's target word count.
const FILLER: &str = "Crawl budget is finite, so every redirect hop and every \
duplicate canonical costs something measurable in coverage. ";

pub fn render_page(graph: &SiteGraph, id: u32, base_url: &str) -> String {
    let node = graph.node(id);
    let mut s = String::with_capacity(4096 + node.word_count as usize * 6);

    s.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n");
    s.push_str("<meta charset=\"utf-8\">\n");
    s.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    let _ = writeln!(s, "<title>{}</title>", node.title);

    if let Some(desc) = &node.meta_description {
        let _ = writeln!(s, "<meta name=\"description\" content=\"{desc}\">");
    }
    if node.noindex {
        s.push_str("<meta name=\"robots\" content=\"noindex, follow\">\n");
    }
    let _ = writeln!(
        s,
        "<link rel=\"canonical\" href=\"{base_url}{}\">",
        node.path
    );
    let _ = writeln!(s, "<meta property=\"og:title\" content=\"{}\">", node.title);
    let _ = writeln!(
        s,
        "<meta property=\"og:url\" content=\"{base_url}{}\">",
        node.path
    );
    s.push_str("</head>\n<body>\n");

    // Navigation, present on every page.
    s.push_str("<nav>\n<ul>\n");
    for &nav_id in &graph.nav {
        let n = graph.node(nav_id);
        let _ = writeln!(s, "<li><a href=\"{}\">{}</a></li>", n.path, n.h1);
    }
    s.push_str("</ul>\n</nav>\n");

    let _ = writeln!(s, "<main>\n<h1>{}</h1>", node.h1);

    // Images. The first `images_missing_alt` of them omit the attribute, which
    // is what the audit rules are meant to catch.
    for i in 0..node.image_count {
        if i < node.images_missing_alt {
            let _ = writeln!(
                s,
                "<img src=\"/static/img-{i}.jpg\" width=\"640\" height=\"360\">"
            );
        } else {
            let _ = writeln!(
                s,
                "<img src=\"/static/img-{i}.jpg\" alt=\"{} illustration {i}\" width=\"640\" height=\"360\">",
                node.h1
            );
        }
    }

    // Body copy sized to the node's word count.
    let words_per_filler = FILLER.split_whitespace().count() as u32;
    let repeats = (node.word_count / words_per_filler).max(1);
    s.push_str("<h2>Overview</h2>\n<p>");
    for _ in 0..repeats {
        s.push_str(FILLER);
    }
    s.push_str("</p>\n");

    // Outlinks.
    s.push_str("<h2>Related</h2>\n<ul>\n");
    for &out in &node.outlinks {
        let target = graph.node(out);
        let _ = writeln!(
            s,
            "<li><a href=\"{}\">{}</a></li>",
            target.path, target.title
        );
    }
    s.push_str("</ul>\n</main>\n");

    s.push_str("<footer><a href=\"/sitemap.xml\">Sitemap</a></footer>\n");
    s.push_str("</body>\n</html>\n");
    s
}
