//! Deterministic site-graph generation.
//!
//! Pure: no I/O, no HTTP, no async. That keeps the property tests fast and
//! makes the graph verifiable independently of the server that serves it.
//!
//! The graph is built as a spanning tree first, then decorated with
//! navigation and cross-links. Building the tree first is what makes
//! "every page is reachable from the root" a structural guarantee rather
//! than something to hope for.

use crate::rng::Rng;
use std::collections::{HashMap, HashSet};

const SECTIONS: [&str; 8] = [
    "products",
    "guides",
    "blog",
    "docs",
    "support",
    "about",
    "pricing",
    "changelog",
];

const WORDS: [&str; 16] = [
    "crawler",
    "index",
    "sitemap",
    "canonical",
    "redirect",
    "latency",
    "throughput",
    "schema",
    "render",
    "budget",
    "cluster",
    "pagination",
    "facet",
    "hreflang",
    "migration",
    "audit",
];

#[derive(Debug, Clone)]
pub struct GraphSpec {
    pub seed: u64,
    pub page_count: u32,
    /// Pages linked from every page, simulating site navigation.
    pub nav_size: u32,
    /// Cross-links added per page on top of its tree children.
    pub extra_links: u32,
    pub max_depth: u16,
}

impl Default for GraphSpec {
    fn default() -> Self {
        Self {
            seed: 0,
            page_count: 100_000,
            nav_size: 6,
            extra_links: 14,
            max_depth: 6,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PageNode {
    pub id: u32,
    pub path: String,
    pub depth: u16,
    pub parent: Option<u32>,
    pub outlinks: Vec<u32>,
    pub title: String,
    pub meta_description: Option<String>,
    pub h1: String,
    pub word_count: u32,
    pub image_count: u8,
    pub images_missing_alt: u8,
    pub noindex: bool,
}

#[derive(Debug)]
pub struct SiteGraph {
    pub spec_seed: u64,
    pub nodes: Vec<PageNode>,
    pub nav: Vec<u32>,
    index: HashMap<String, u32>,
}

impl SiteGraph {
    pub fn generate(spec: &GraphSpec) -> Self {
        assert!(spec.page_count > 0, "page_count must be at least 1");
        let mut rng = Rng::new(spec.seed);
        let n = spec.page_count;

        // Pass 1: spanning tree, so connectivity is structural.
        let mut nodes: Vec<PageNode> = Vec::with_capacity(n as usize);
        nodes.push(PageNode {
            id: 0,
            path: "/".to_string(),
            depth: 0,
            parent: None,
            outlinks: Vec::new(),
            title: "Home".to_string(),
            meta_description: Some("The home page of the fixture site.".to_string()),
            h1: "Home".to_string(),
            word_count: 420,
            image_count: 2,
            images_missing_alt: 0,
            noindex: false,
        });

        for id in 1..n {
            // Parent is any earlier node not already at max depth.
            let mut parent = rng.below(id);
            let mut guard = 0;
            while nodes[parent as usize].depth >= spec.max_depth && guard < 32 {
                parent = rng.below(id);
                guard += 1;
            }
            if nodes[parent as usize].depth >= spec.max_depth {
                parent = 0; // fall back to root rather than exceed max_depth
            }

            let depth = nodes[parent as usize].depth + 1;
            let section = SECTIONS[rng.below(SECTIONS.len() as u32) as usize];
            let word = WORDS[rng.below(WORDS.len() as u32) as usize];
            // The -{id} suffix makes path collisions impossible.
            let path = if depth == 1 {
                format!("/{section}/{word}-{id}")
            } else {
                format!(
                    "{}/{word}-{id}",
                    nodes[parent as usize].path.trim_end_matches('/')
                )
            };

            let has_desc = !rng.chance(12); // ~12% missing meta description
            let noindex = rng.chance(3);
            let image_count = rng.below(6) as u8;
            let images_missing_alt = if image_count > 0 && rng.chance(25) {
                (rng.below(u32::from(image_count)) as u8 + 1).min(image_count)
            } else {
                0
            };

            nodes.push(PageNode {
                id,
                path,
                depth,
                parent: Some(parent),
                outlinks: Vec::new(),
                title: format!("{} {} — {}", capitalize(word), id, capitalize(section)),
                meta_description: has_desc.then(|| {
                    format!("A fixture page about {word} in the {section} section, page {id}.")
                }),
                h1: format!("{} {}", capitalize(word), id),
                word_count: 120 + rng.below(1800),
                image_count,
                images_missing_alt,
                noindex,
            });
        }

        // Pass 2: navigation, present on every page.
        let nav: Vec<u32> = (1..=spec.nav_size.min(n.saturating_sub(1))).collect();

        // Pass 3: outlinks — tree children, then nav, then cross-links.
        let mut children: Vec<Vec<u32>> = vec![Vec::new(); n as usize];
        for node in &nodes {
            if let Some(p) = node.parent {
                children[p as usize].push(node.id);
            }
        }

        for id in 0..n {
            let mut out = std::mem::take(&mut children[id as usize]);
            // A set alongside the vec keeps dedup O(1); a linear `contains`
            // scan here made 100k-page generation quadratic.
            let mut seen: HashSet<u32> = out.iter().copied().collect();
            seen.insert(id);

            for &nav_id in &nav {
                if seen.insert(nav_id) {
                    out.push(nav_id);
                }
            }
            for _ in 0..spec.extra_links {
                let target = rng.below(n);
                if seen.insert(target) {
                    out.push(target);
                }
            }
            nodes[id as usize].outlinks = out;
        }

        let index = nodes
            .iter()
            .map(|node| (node.path.clone(), node.id))
            .collect();
        SiteGraph {
            spec_seed: spec.seed,
            nodes,
            nav,
            index,
        }
    }

    pub fn lookup(&self, path: &str) -> Option<u32> {
        self.index.get(path).copied()
    }

    pub fn node(&self, id: u32) -> &PageNode {
        &self.nodes[id as usize]
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}
