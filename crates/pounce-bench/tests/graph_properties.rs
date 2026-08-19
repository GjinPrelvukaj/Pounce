use pounce_bench::graph::{GraphSpec, SiteGraph};
use std::collections::HashSet;

fn spec(pages: u32) -> GraphSpec {
    GraphSpec {
        seed: 1234,
        page_count: pages,
        ..GraphSpec::default()
    }
}

#[test]
fn same_seed_produces_identical_graph() {
    let a = SiteGraph::generate(&spec(500));
    let b = SiteGraph::generate(&spec(500));
    assert_eq!(a.nodes.len(), b.nodes.len());
    for (x, y) in a.nodes.iter().zip(b.nodes.iter()) {
        assert_eq!(x.path, y.path);
        assert_eq!(x.outlinks, y.outlinks);
        assert_eq!(x.title, y.title);
    }
}

#[test]
fn different_seed_produces_different_graph() {
    let a = SiteGraph::generate(&spec(500));
    let b = SiteGraph::generate(&GraphSpec {
        seed: 9999,
        ..spec(500)
    });
    assert_ne!(
        a.nodes.iter().map(|n| n.path.clone()).collect::<Vec<_>>(),
        b.nodes.iter().map(|n| n.path.clone()).collect::<Vec<_>>()
    );
}

#[test]
fn produces_exactly_the_requested_page_count() {
    let g = SiteGraph::generate(&spec(1000));
    assert_eq!(g.nodes.len(), 1000);
}

#[test]
fn root_is_first_and_is_slash() {
    let g = SiteGraph::generate(&spec(50));
    assert_eq!(g.nodes[0].path, "/");
    assert_eq!(g.nodes[0].depth, 0);
    assert_eq!(g.nodes[0].parent, None);
}

#[test]
fn every_page_is_reachable_from_root() {
    let g = SiteGraph::generate(&spec(2000));
    let mut seen = HashSet::new();
    let mut stack = vec![0u32];
    seen.insert(0u32);
    while let Some(id) = stack.pop() {
        for &out in &g.nodes[id as usize].outlinks {
            if seen.insert(out) {
                stack.push(out);
            }
        }
    }
    assert_eq!(seen.len(), g.nodes.len(), "orphaned pages exist");
}

#[test]
fn all_paths_are_unique() {
    let g = SiteGraph::generate(&spec(3000));
    let unique: HashSet<&str> = g.nodes.iter().map(|n| n.path.as_str()).collect();
    assert_eq!(unique.len(), g.nodes.len(), "duplicate paths generated");
}

#[test]
fn outlinks_never_point_outside_the_graph() {
    let g = SiteGraph::generate(&spec(500));
    let n = g.nodes.len() as u32;
    for node in &g.nodes {
        for &out in &node.outlinks {
            assert!(out < n, "dangling link {out} on {}", node.path);
        }
    }
}

#[test]
fn no_page_links_to_itself() {
    let g = SiteGraph::generate(&spec(1000));
    for node in &g.nodes {
        assert!(
            !node.outlinks.contains(&node.id),
            "{} links to itself",
            node.path
        );
    }
}

#[test]
fn outlinks_contain_no_duplicates() {
    let g = SiteGraph::generate(&spec(500));
    for node in &g.nodes {
        let unique: HashSet<u32> = node.outlinks.iter().copied().collect();
        assert_eq!(
            unique.len(),
            node.outlinks.len(),
            "{} has duplicate outlinks",
            node.path
        );
    }
}

#[test]
fn depth_never_exceeds_the_configured_maximum() {
    let s = GraphSpec {
        max_depth: 4,
        ..spec(2000)
    };
    let g = SiteGraph::generate(&s);
    assert!(g.nodes.iter().all(|n| n.depth <= 4));
}

#[test]
fn link_density_is_realistic() {
    let g = SiteGraph::generate(&spec(1000));
    let total: usize = g.nodes.iter().map(|n| n.outlinks.len()).sum();
    let mean = total as f64 / g.nodes.len() as f64;
    assert!(
        (10.0..=80.0).contains(&mean),
        "mean outlinks {mean} is unrealistic"
    );
}

#[test]
fn some_pages_carry_seo_defects() {
    // The fixture must contain real problems, or the audit rules that
    // consume it in v0.1 have nothing to find.
    let g = SiteGraph::generate(&spec(1000));
    assert!(g.nodes.iter().any(|n| n.meta_description.is_none()));
    assert!(g.nodes.iter().any(|n| n.noindex));
    assert!(g.nodes.iter().any(|n| n.images_missing_alt > 0));
}

#[test]
fn images_missing_alt_never_exceeds_image_count() {
    let g = SiteGraph::generate(&spec(2000));
    for node in &g.nodes {
        assert!(
            node.images_missing_alt <= node.image_count,
            "{} claims {} of {} images lack alt",
            node.path,
            node.images_missing_alt,
            node.image_count
        );
    }
}

#[test]
fn lookup_resolves_paths_to_nodes() {
    let g = SiteGraph::generate(&spec(200));
    let target = &g.nodes[137];
    assert_eq!(g.lookup(&target.path), Some(137));
    assert_eq!(g.lookup("/definitely-not-a-real-path"), None);
}

#[test]
fn generates_a_hundred_thousand_pages_without_quadratic_blowup() {
    // The generous bound is deliberate: it is not a performance assertion,
    // it is a shape assertion. Quadratic dedup would take minutes here.
    let start = std::time::Instant::now();
    let g = SiteGraph::generate(&spec(100_000));
    let elapsed = start.elapsed();

    assert_eq!(g.nodes.len(), 100_000);
    assert!(
        elapsed.as_secs() < 20,
        "100k pages took {elapsed:?} — dedup is probably quadratic"
    );
    eprintln!("generated 100k pages in {elapsed:?}");
}

#[test]
fn single_page_graph_is_just_the_root() {
    let g = SiteGraph::generate(&spec(1));
    assert_eq!(g.nodes.len(), 1);
    assert_eq!(g.nodes[0].path, "/");
    assert!(g.nodes[0].outlinks.is_empty());
}
