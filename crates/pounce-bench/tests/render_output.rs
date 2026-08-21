use pounce_bench::graph::{GraphSpec, SiteGraph};
use pounce_bench::render::render_page;

const BASE: &str = "http://localhost:8080";

fn graph() -> SiteGraph {
    SiteGraph::generate(&GraphSpec {
        seed: 77,
        page_count: 300,
        ..GraphSpec::default()
    })
}

/// Counts `<img …>` tags that carry no `alt` attribute. Scoped per tag so an
/// `alt` appearing anywhere else in the document cannot skew the result.
fn imgs_without_alt(html: &str) -> usize {
    html.match_indices("<img ")
        .filter(|(i, _)| {
            let tag_end = html[*i..].find('>').map(|e| i + e).unwrap_or(html.len());
            !html[*i..tag_end].contains("alt=")
        })
        .count()
}

#[test]
fn renders_a_complete_html_document() {
    let g = graph();
    let html = render_page(&g, 5, BASE);
    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(html.contains("<html lang=\"en\">"));
    assert!(html.trim_end().ends_with("</html>"));
}

#[test]
fn includes_title_and_h1() {
    let g = graph();
    let node = g.node(5);
    let html = render_page(&g, 5, BASE);
    assert!(html.contains(&format!("<title>{}</title>", node.title)));
    assert!(html.contains(&format!("<h1>{}</h1>", node.h1)));
}

#[test]
fn emits_absolute_canonical() {
    let g = graph();
    let node = g.node(9);
    let html = render_page(&g, 9, BASE);
    let expected = format!("<link rel=\"canonical\" href=\"{BASE}{}\">", node.path);
    assert!(
        html.contains(&expected),
        "canonical missing or not absolute"
    );
}

#[test]
fn omits_meta_description_when_the_node_has_none() {
    let g = graph();
    let missing = g
        .nodes
        .iter()
        .find(|n| n.meta_description.is_none())
        .expect("fixture should contain a page without a description");
    let html = render_page(&g, missing.id, BASE);
    assert!(!html.contains("name=\"description\""));
}

#[test]
fn emits_noindex_only_for_noindex_pages() {
    let g = graph();
    let noindexed = g
        .nodes
        .iter()
        .find(|n| n.noindex)
        .expect("fixture should contain a noindex page");
    let indexed = g
        .nodes
        .iter()
        .find(|n| !n.noindex)
        .expect("fixture should contain an indexable page");
    assert!(render_page(&g, noindexed.id, BASE).contains("content=\"noindex, follow\""));
    assert!(!render_page(&g, indexed.id, BASE).contains("noindex"));
}

#[test]
fn renders_every_outlink_as_a_relative_anchor() {
    let g = graph();
    let node = g.node(12);
    let html = render_page(&g, 12, BASE);
    for &out in &node.outlinks {
        let href = format!("href=\"{}\"", g.node(out).path);
        assert!(html.contains(&href), "missing link to {}", g.node(out).path);
    }
}

#[test]
fn images_without_alt_match_the_node() {
    let g = graph();
    let node = g
        .nodes
        .iter()
        .find(|n| n.images_missing_alt > 0)
        .expect("fixture should contain images without alt");
    let html = render_page(&g, node.id, BASE);
    assert_eq!(imgs_without_alt(&html), node.images_missing_alt as usize);
}

#[test]
fn pages_with_full_alt_coverage_have_no_bare_images() {
    let g = graph();
    let node = g
        .nodes
        .iter()
        .find(|n| n.image_count > 0 && n.images_missing_alt == 0)
        .expect("fixture should contain a page with complete alt coverage");
    let html = render_page(&g, node.id, BASE);
    assert_eq!(imgs_without_alt(&html), 0);
}

#[test]
fn renders_every_image_the_node_declares() {
    let g = graph();
    let node = g
        .nodes
        .iter()
        .find(|n| n.image_count >= 3)
        .expect("fixture should have an image-rich page");
    let html = render_page(&g, node.id, BASE);
    assert_eq!(html.matches("<img ").count(), node.image_count as usize);
}

#[test]
fn body_length_tracks_word_count() {
    let g = graph();
    let short = g.nodes.iter().min_by_key(|n| n.word_count).unwrap();
    let long = g.nodes.iter().max_by_key(|n| n.word_count).unwrap();
    assert!(render_page(&g, long.id, BASE).len() > render_page(&g, short.id, BASE).len());
}

#[test]
fn output_is_deterministic() {
    let g = graph();
    assert_eq!(render_page(&g, 42, BASE), render_page(&g, 42, BASE));
}

#[test]
fn navigation_appears_on_every_page() {
    let g = graph();
    for id in [0u32, 7, 88, 299] {
        let html = render_page(&g, id, BASE);
        assert!(html.contains("<nav>"), "page {id} has no nav");
    }
}

// ---- external links and nofollow (M2 prerequisite) ----------------------

#[test]
fn renders_external_links_as_absolute_anchors() {
    let graph = SiteGraph::generate(&GraphSpec {
        seed: 42,
        page_count: 200,
        ..GraphSpec::default()
    });
    let id = graph
        .nodes
        .iter()
        .position(|n| !n.external.is_empty())
        .expect("some page has an external link") as u32;
    let html = render_page(&graph, id, "http://localhost:8080");
    for url in &graph.node(id).external {
        assert!(html.contains(&format!("href=\"{url}\"")), "missing {url}");
    }
}

#[test]
fn renders_rel_nofollow_on_the_marked_outlinks() {
    let graph = SiteGraph::generate(&GraphSpec {
        seed: 42,
        page_count: 200,
        ..GraphSpec::default()
    });
    let id = graph
        .nodes
        .iter()
        .position(|n| n.nofollow_outlinks > 0)
        .expect("some page marks a nofollow link") as u32;
    let html = render_page(&graph, id, "http://localhost:8080");
    let count = html.matches("rel=\"nofollow\"").count();
    assert_eq!(count, graph.node(id).nofollow_outlinks as usize);
}
