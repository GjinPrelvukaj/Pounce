# Phase 0 — Benchmark Harness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a deterministic fixture website and a benchmark runner that measures crawl throughput, peak memory, and wall time identically for Pounce and every competitor — so that the project's central speed claim can be verified before any crawler code is written.

**Architecture:** A single crate, `pounce-bench`, containing three separable pieces: a deterministic site-graph generator (pure, no I/O), an axum server that renders that graph as HTML over HTTP, and a runner that spawns an arbitrary crawler process against the server while sampling its memory. The graph generator is pure and seeded, so the same seed always produces a byte-identical site — that is what makes benchmark numbers comparable across machines and across months.

**Tech Stack:** Rust 2024 edition, axum 0.8, tokio 1.53, sysinfo 0.39, clap 4.6, criterion 0.8, lol_html 3.0, serde_json.

**Spec:** [`docs/specs/2026-08-19-pounce-design.md`](../specs/2026-08-19-pounce-design.md) §7 Phase 0.0, §8 Performance targets.

---

## Why this is Phase 0

The entire product positioning rests on one claim: Pounce is faster than the incumbents. If that claim is false, everything downstream is wasted effort. This harness exists to test the premise in two weeks rather than six months. It is also permanent infrastructure — the fixture site becomes the integration test suite in v0.1, and the criterion benches become the CI regression gate.

**Success criteria for Phase 0:** you can run one command, and get a table comparing Pounce (or, initially, any existing crawler) against competitors on URLs/sec and peak RSS, reproducibly, on a site nobody had to host.

---

## File Structure

```
Cargo.toml                            workspace root, shared dependency versions
rust-toolchain.toml                   pinned toolchain
.gitignore
.github/workflows/ci.yml              fmt, clippy, test, bench-regression

crates/pounce-bench/
├─ Cargo.toml
├─ src/
│  ├─ lib.rs                          re-exports; crate docs
│  ├─ rng.rs                          SplitMix64 — deterministic, version-independent
│  ├─ graph.rs                        GraphSpec, PageNode, SiteGraph generation
│  ├─ render.rs                       PageNode → HTML string
│  ├─ pathological.rs                 handlers for redirect/slow/huge/malformed cases
│  ├─ server.rs                       axum Router assembly, robots.txt, sitemap.xml
│  ├─ metrics.rs                      child-process spawner + RSS sampler
│  └─ report.rs                       BenchResult, JSON + markdown table output
├─ src/bin/
│  ├─ fixture_site.rs                 standalone server binary
│  └─ bench_runner.rs                 the runner CLI
├─ benches/
│  └─ parse.rs                        criterion: lol_html throughput
└─ tests/
   ├─ graph_properties.rs             determinism, connectivity, density
   ├─ render_output.rs                HTML correctness
   └─ server_integration.rs           live HTTP against a spawned server
```

**Responsibility boundaries:** `graph.rs` is pure and has no dependency on axum or tokio — it can be property-tested at speed. `render.rs` depends only on `graph.rs`. `server.rs` is the only module that knows about HTTP. `metrics.rs` knows nothing about crawling — it spawns a process and watches it. This separation is what lets each piece be tested without standing up a server.

---

## Task 1: Workspace scaffold

**Files:**
- Create: `Cargo.toml`
- Create: `rust-toolchain.toml`
- Create: `.gitignore`
- Create: `crates/pounce-bench/Cargo.toml`
- Create: `crates/pounce-bench/src/lib.rs`

- [ ] **Step 1: Create the workspace root manifest**

`Cargo.toml`:

```toml
[workspace]
resolver = "3"
members = ["crates/*"]

[workspace.package]
version = "0.0.1"
edition = "2024"
publish = false          # proprietary — nothing goes to crates.io
repository = "https://github.com/GjinPrelvukaj/Pounce"
rust-version = "1.97"

[workspace.dependencies]
anyhow = "1.0"
axum = "0.8"
clap = { version = "4.6", features = ["derive"] }
criterion = "0.8"
lol_html = "3.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
sysinfo = "0.39"
tokio = { version = "1.53", features = ["rt-multi-thread", "macros", "net", "time", "signal"] }
tower = "0.5"
reqwest = { version = "0.13", default-features = false, features = ["rustls", "gzip"] }

[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1

[profile.bench]
inherits = "release"
debug = true
```

- [ ] **Step 2: Pin the toolchain**

`rust-toolchain.toml`:

```toml
[toolchain]
channel = "1.97"
components = ["rustfmt", "clippy"]
```

- [ ] **Step 3: Create `.gitignore`**

```
/target
**/*.rs.bk
*.pounce
/bench-results
.DS_Store
```

- [ ] **Step 4: Create the bench crate manifest**

`crates/pounce-bench/Cargo.toml`:

```toml
[package]
name = "pounce-bench"
version.workspace = true
edition.workspace = true
repository.workspace = true
description = "Deterministic fixture site and benchmark runner for Pounce"
publish = false

[dependencies]
anyhow.workspace = true
axum.workspace = true
clap.workspace = true
serde.workspace = true
serde_json.workspace = true
sysinfo.workspace = true
tokio.workspace = true

[dev-dependencies]
criterion.workspace = true
lol_html.workspace = true
reqwest.workspace = true
tower.workspace = true

[[bin]]
name = "fixture-site"
path = "src/bin/fixture_site.rs"

[[bin]]
name = "bench-runner"
path = "src/bin/bench_runner.rs"

[[bench]]
name = "parse"
harness = false
```

- [ ] **Step 5: Create a minimal lib.rs so the crate compiles**

`crates/pounce-bench/src/lib.rs`:

```rust
//! Deterministic fixture site and benchmark harness for Pounce.
//!
//! The fixture site is generated from a seed, so the same `GraphSpec`
//! always produces a byte-identical website. That reproducibility is what
//! makes benchmark numbers comparable across machines and over time.

pub mod rng;
```

Create `crates/pounce-bench/src/rng.rs` as an empty file for now so the module resolves:

```rust
// Implemented in Task 2.
```

- [ ] **Step 6: Verify the workspace builds**

Run: `cargo build`
Expected: `Finished dev profile` with no errors. The two `[[bin]]` targets will fail because their files don't exist yet — if so, temporarily comment out the `[[bin]]` and `[[bench]]` sections, confirm `cargo build` succeeds, then restore them. They are filled in by Tasks 8 and 9.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml rust-toolchain.toml .gitignore crates/
git commit -m "chore: scaffold cargo workspace and pounce-bench crate"
```

---

## Task 2: Deterministic PRNG

A hand-rolled SplitMix64. Using the `rand` crate would make fixtures depend on `rand`'s internal algorithm, which changes between major versions — a fixture site that silently changes shape when a dependency updates would invalidate every historical benchmark. Twenty lines of our own code buys permanent reproducibility.

**Files:**
- Modify: `crates/pounce-bench/src/rng.rs`
- Test: inline `#[cfg(test)]` module in the same file

- [ ] **Step 1: Write the failing tests**

Replace the contents of `crates/pounce-bench/src/rng.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_yields_same_sequence() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        let seq_a: Vec<u64> = (0..100).map(|_| a.next_u64()).collect();
        let seq_b: Vec<u64> = (0..100).map(|_| b.next_u64()).collect();
        assert_eq!(seq_a, seq_b);
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = Rng::new(1);
        let mut b = Rng::new(2);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn below_stays_in_range() {
        let mut r = Rng::new(7);
        for _ in 0..1000 {
            let v = r.below(10);
            assert!(v < 10, "below(10) returned {v}");
        }
    }

    #[test]
    fn below_one_is_always_zero() {
        let mut r = Rng::new(9);
        assert_eq!(r.below(1), 0);
    }

    #[test]
    fn known_vector_is_stable() {
        // Locks the algorithm. If this test ever fails, every previously
        // published benchmark became incomparable — treat as a breaking change.
        let mut r = Rng::new(0);
        assert_eq!(r.next_u64(), 16294208416658607535);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p pounce-bench rng`
Expected: FAIL — `cannot find type Rng in this scope`.

- [ ] **Step 3: Write the implementation**

Insert above the `#[cfg(test)]` module in `crates/pounce-bench/src/rng.rs`:

```rust
//! A seeded SplitMix64 generator.
//!
//! Deliberately hand-rolled rather than pulling in `rand`: fixture
//! reproducibility must not depend on a third-party crate's internal
//! algorithm, which is free to change across major versions.

/// Deterministic pseudo-random generator. Not cryptographically secure,
/// and must never be used for anything security-relevant.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform-ish value in `0..n`. Returns 0 when `n == 0`.
    pub fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        (self.next_u64() % u64::from(n)) as u32
    }

    /// Returns true with probability `percent / 100`.
    pub fn chance(&mut self, percent: u32) -> bool {
        self.below(100) < percent
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p pounce-bench rng`
Expected: PASS, 5 tests.

If `known_vector_is_stable` fails, **do not change the assertion to match your output** — that defeats the test's purpose. Verify the constants against the reference SplitMix64 algorithm first. If the implementation is correct and the expected value in the plan is wrong, update the expected value and note it in the commit message.

- [ ] **Step 5: Commit**

```bash
git add crates/pounce-bench/src/rng.rs
git commit -m "feat(bench): add deterministic SplitMix64 generator"
```

---

## Task 3: Site graph generation

**Files:**
- Create: `crates/pounce-bench/src/graph.rs`
- Modify: `crates/pounce-bench/src/lib.rs`
- Test: `crates/pounce-bench/tests/graph_properties.rs`

The graph is a spanning tree (guaranteeing every page is reachable from the root) plus cross-links and a site-wide nav. Building the tree first is what makes connectivity a structural guarantee rather than something to hope for.

- [ ] **Step 1: Write the failing tests**

`crates/pounce-bench/tests/graph_properties.rs`:

```rust
use pounce_bench::graph::{GraphSpec, SiteGraph};
use std::collections::HashSet;

fn spec(pages: u32) -> GraphSpec {
    GraphSpec { seed: 1234, page_count: pages, ..GraphSpec::default() }
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
    let b = SiteGraph::generate(&GraphSpec { seed: 9999, ..spec(500) });
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
fn depth_never_exceeds_the_configured_maximum() {
    let s = GraphSpec { max_depth: 4, ..spec(2000) };
    let g = SiteGraph::generate(&s);
    assert!(g.nodes.iter().all(|n| n.depth <= 4));
}

#[test]
fn link_density_is_realistic() {
    let g = SiteGraph::generate(&spec(1000));
    let total: usize = g.nodes.iter().map(|n| n.outlinks.len()).sum();
    let mean = total as f64 / g.nodes.len() as f64;
    assert!(mean >= 10.0 && mean <= 80.0, "mean outlinks {mean} is unrealistic");
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
fn lookup_resolves_paths_to_nodes() {
    let g = SiteGraph::generate(&spec(200));
    let target = &g.nodes[137];
    assert_eq!(g.lookup(&target.path), Some(137));
    assert_eq!(g.lookup("/definitely-not-a-real-path"), None);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p pounce-bench --test graph_properties`
Expected: FAIL — `unresolved import pounce_bench::graph`.

- [ ] **Step 3: Write the implementation**

`crates/pounce-bench/src/graph.rs`:

```rust
//! Deterministic site-graph generation.
//!
//! Pure: no I/O, no HTTP, no async. That keeps property tests fast and
//! makes the graph independently verifiable.

use crate::rng::Rng;
use std::collections::HashMap;

const SECTIONS: [&str; 8] = [
    "products", "guides", "blog", "docs", "support", "about", "pricing", "changelog",
];

const WORDS: [&str; 16] = [
    "crawler", "index", "sitemap", "canonical", "redirect", "latency", "throughput",
    "schema", "render", "budget", "cluster", "pagination", "facet", "hreflang",
    "migration", "audit",
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
        Self { seed: 0, page_count: 100_000, nav_size: 6, extra_links: 14, max_depth: 6 }
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

        // Pass 1: build a spanning tree so connectivity is structural.
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
            // Parent is any earlier node that is not already at max depth.
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
            let path = if depth == 1 {
                format!("/{section}/{word}-{id}")
            } else {
                format!("{}/{word}-{id}", nodes[parent as usize].path.trim_end_matches('/'))
            };

            let has_desc = !rng.chance(12); // ~12% missing meta description
            let noindex = rng.chance(3);
            let image_count = rng.below(6) as u8;
            let images_missing_alt =
                if image_count > 0 && rng.chance(25) { rng.below(u32::from(image_count)) as u8 + 1 } else { 0 };

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
                images_missing_alt: images_missing_alt.min(image_count),
                noindex,
            });
        }

        // Pass 2: nav links, present on every page.
        let nav: Vec<u32> = (1..=spec.nav_size.min(n.saturating_sub(1))).collect();

        // Pass 3: wire outlinks — children, nav, then cross-links.
        let mut children: Vec<Vec<u32>> = vec![Vec::new(); n as usize];
        for node in &nodes {
            if let Some(p) = node.parent {
                children[p as usize].push(node.id);
            }
        }

        for id in 0..n {
            let mut out = children[id as usize].clone();
            for &nav_id in &nav {
                if nav_id != id && !out.contains(&nav_id) {
                    out.push(nav_id);
                }
            }
            for _ in 0..spec.extra_links {
                let target = rng.below(n);
                if target != id && !out.contains(&target) {
                    out.push(target);
                }
            }
            nodes[id as usize].outlinks = out;
        }

        let index = nodes.iter().map(|node| (node.path.clone(), node.id)).collect();
        SiteGraph { spec_seed: spec.seed, nodes, nav, index }
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
```

- [ ] **Step 4: Register the module**

In `crates/pounce-bench/src/lib.rs`, add below the existing `pub mod rng;`:

```rust
pub mod graph;
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p pounce-bench --test graph_properties`
Expected: PASS, 11 tests.

The most likely failure is `all_paths_are_unique` — the `-{id}` suffix in every path makes collisions impossible, so if it fails, the path construction has been altered. The second most likely is `every_page_is_reachable_from_root`, which can only fail if the tree pass was changed.

- [ ] **Step 6: Verify generation speed at scale**

Run: `cargo run --release -p pounce-bench --example none 2>/dev/null; cargo test -p pounce-bench --release --test graph_properties -- --nocapture`
Expected: PASS in under 5 seconds. Generation of 100k pages must stay under ~2s or fixture startup becomes annoying; if it is slower, the `out.contains(&target)` linear scan is the culprit and should become a small `HashSet` per node.

- [ ] **Step 7: Commit**

```bash
git add crates/pounce-bench/src/graph.rs crates/pounce-bench/src/lib.rs crates/pounce-bench/tests/graph_properties.rs
git commit -m "feat(bench): generate deterministic site graphs with seeded connectivity"
```

---

## Task 4: HTML rendering

**Files:**
- Create: `crates/pounce-bench/src/render.rs`
- Modify: `crates/pounce-bench/src/lib.rs`
- Test: `crates/pounce-bench/tests/render_output.rs`

- [ ] **Step 1: Write the failing tests**

`crates/pounce-bench/tests/render_output.rs`:

```rust
use pounce_bench::graph::{GraphSpec, SiteGraph};
use pounce_bench::render::render_page;

fn graph() -> SiteGraph {
    SiteGraph::generate(&GraphSpec { seed: 77, page_count: 300, ..GraphSpec::default() })
}

#[test]
fn renders_a_complete_html_document() {
    let g = graph();
    let html = render_page(&g, 5, "http://localhost:8080");
    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(html.contains("<html lang=\"en\">"));
    assert!(html.trim_end().ends_with("</html>"));
}

#[test]
fn includes_title_and_h1() {
    let g = graph();
    let node = g.node(5);
    let html = render_page(&g, 5, "http://localhost:8080");
    assert!(html.contains(&format!("<title>{}</title>", node.title)));
    assert!(html.contains(&format!("<h1>{}</h1>", node.h1)));
}

#[test]
fn emits_absolute_canonical() {
    let g = graph();
    let node = g.node(9);
    let html = render_page(&g, 9, "http://localhost:8080");
    let expected = format!(
        "<link rel=\"canonical\" href=\"http://localhost:8080{}\">",
        node.path
    );
    assert!(html.contains(&expected), "canonical missing or relative");
}

#[test]
fn omits_meta_description_when_the_node_has_none() {
    let g = graph();
    let missing = g.nodes.iter().find(|n| n.meta_description.is_none()).expect("fixture should contain a page without a description");
    let html = render_page(&g, missing.id, "http://localhost:8080");
    assert!(!html.contains("name=\"description\""));
}

#[test]
fn emits_noindex_only_for_noindex_pages() {
    let g = graph();
    let noindexed = g.nodes.iter().find(|n| n.noindex).expect("fixture should contain a noindex page");
    let indexed = g.nodes.iter().find(|n| !n.noindex).expect("fixture should contain an indexable page");
    assert!(render_page(&g, noindexed.id, "http://localhost:8080").contains("content=\"noindex, follow\""));
    assert!(!render_page(&g, indexed.id, "http://localhost:8080").contains("noindex"));
}

#[test]
fn renders_every_outlink_as_a_relative_anchor() {
    let g = graph();
    let node = g.node(12);
    let html = render_page(&g, 12, "http://localhost:8080");
    for &out in &node.outlinks {
        let href = format!("href=\"{}\"", g.node(out).path);
        assert!(html.contains(&href), "missing link to {}", g.node(out).path);
    }
}

#[test]
fn images_without_alt_are_actually_missing_alt() {
    let g = graph();
    let node = g.nodes.iter().find(|n| n.images_missing_alt > 0).expect("fixture should contain images without alt");
    let html = render_page(&g, node.id, "http://localhost:8080");
    let without_alt = html.matches("<img src=\"/static/img-").count()
        - html.matches("alt=\"").count();
    assert_eq!(without_alt as u8, node.images_missing_alt);
}

#[test]
fn body_length_tracks_word_count() {
    let g = graph();
    let short = g.nodes.iter().min_by_key(|n| n.word_count).unwrap();
    let long = g.nodes.iter().max_by_key(|n| n.word_count).unwrap();
    assert!(
        render_page(&g, long.id, "http://localhost:8080").len()
            > render_page(&g, short.id, "http://localhost:8080").len()
    );
}

#[test]
fn output_is_deterministic() {
    let g = graph();
    assert_eq!(
        render_page(&g, 42, "http://localhost:8080"),
        render_page(&g, 42, "http://localhost:8080")
    );
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p pounce-bench --test render_output`
Expected: FAIL — `unresolved import pounce_bench::render`.

- [ ] **Step 3: Write the implementation**

`crates/pounce-bench/src/render.rs`:

```rust
//! Renders a `PageNode` into an HTML document.
//!
//! Output is deliberately ordinary: the goal is markup that exercises a
//! crawler the way a real CMS would, not markup that shows off.

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
    let _ = writeln!(s, "<link rel=\"canonical\" href=\"{base_url}{}\">", node.path);
    let _ = writeln!(s, "<meta property=\"og:title\" content=\"{}\">", node.title);
    let _ = writeln!(s, "<meta property=\"og:url\" content=\"{base_url}{}\">", node.path);
    s.push_str("</head>\n<body>\n");

    // Navigation, present on every page.
    s.push_str("<nav>\n<ul>\n");
    for &nav_id in &graph.nav {
        let n = graph.node(nav_id);
        let _ = writeln!(s, "<li><a href=\"{}\">{}</a></li>", n.path, n.h1);
    }
    s.push_str("</ul>\n</nav>\n");

    let _ = writeln!(s, "<main>\n<h1>{}</h1>", node.h1);

    // Images: those counted in `images_missing_alt` omit the attribute.
    for i in 0..node.image_count {
        if i < node.images_missing_alt {
            let _ = writeln!(s, "<img src=\"/static/img-{i}.jpg\" width=\"640\" height=\"360\">");
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
        let _ = writeln!(s, "<li><a href=\"{}\">{}</a></li>", target.path, target.title);
    }
    s.push_str("</ul>\n</main>\n");

    s.push_str("<footer><a href=\"/sitemap.xml\">Sitemap</a></footer>\n");
    s.push_str("</body>\n</html>\n");
    s
}
```

- [ ] **Step 4: Register the module**

Add to `crates/pounce-bench/src/lib.rs`:

```rust
pub mod render;
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p pounce-bench --test render_output`
Expected: PASS, 9 tests.

`images_without_alt_are_actually_missing_alt` is the fragile one — it counts `alt="` occurrences across the whole document. If the nav or footer ever gains an `alt` attribute, that test needs to count within `<img` tags specifically instead.

- [ ] **Step 6: Commit**

```bash
git add crates/pounce-bench/src/render.rs crates/pounce-bench/src/lib.rs crates/pounce-bench/tests/render_output.rs
git commit -m "feat(bench): render site graph nodes as HTML documents"
```

---

## Task 5: Pathological endpoints

These are the cases that break naive crawlers. Building them now means v0.1's crawler is tested against them from its first commit rather than discovering them in production.

**Files:**
- Create: `crates/pounce-bench/src/pathological.rs`
- Modify: `crates/pounce-bench/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

Create `crates/pounce-bench/src/pathological.rs` containing only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirect_chain_advances_until_it_terminates() {
        assert_eq!(redirect_chain_target(5), Redirect::To("/redirect-chain/4".into()));
        assert_eq!(redirect_chain_target(1), Redirect::To("/redirect-chain/0".into()));
        assert_eq!(redirect_chain_target(0), Redirect::Terminal);
    }

    #[test]
    fn redirect_loop_always_points_back_into_the_loop() {
        assert_eq!(redirect_loop_target(0, 3), "/redirect-loop/3/1");
        assert_eq!(redirect_loop_target(1, 3), "/redirect-loop/3/2");
        assert_eq!(redirect_loop_target(2, 3), "/redirect-loop/3/0");
    }

    #[test]
    fn malformed_html_is_actually_malformed_but_has_recoverable_links() {
        let html = malformed_html();
        assert!(html.contains("<a href=\"/malformed-target-1\""));
        assert!(html.contains("<p>unclosed"));
        assert!(!html.contains("</html>"));
    }

    #[test]
    fn huge_page_hits_the_requested_size() {
        let html = huge_html(2);
        let mb = html.len() as f64 / (1024.0 * 1024.0);
        assert!(mb >= 2.0 && mb < 2.5, "expected ~2MB, got {mb}MB");
    }

    #[test]
    fn size_and_hop_requests_are_clamped() {
        // A crawler asking for /huge/9999 must not be able to OOM the fixture.
        assert!(huge_html(9999).len() <= 16 * 1024 * 1024);
        assert_eq!(clamp_hops(9999), MAX_HOPS);
        assert_eq!(clamp_delay_ms(999_999), MAX_DELAY_MS);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p pounce-bench pathological`
Expected: FAIL — `cannot find function redirect_chain_target`.

- [ ] **Step 3: Write the implementation**

Insert above the `#[cfg(test)]` module in `crates/pounce-bench/src/pathological.rs`:

```rust
//! Endpoints that exercise the failure modes real crawlers hit:
//! redirect chains and loops, slow responses, oversized bodies, and
//! markup that does not parse.
//!
//! Every size and count is clamped so that a misbehaving crawler cannot
//! exhaust the fixture server's memory or wedge it indefinitely.

pub const MAX_HOPS: u32 = 20;
pub const MAX_DELAY_MS: u64 = 30_000;
pub const MAX_HUGE_MB: usize = 16;

#[derive(Debug, PartialEq, Eq)]
pub enum Redirect {
    To(String),
    Terminal,
}

/// A finite chain: `/redirect-chain/N` → `/redirect-chain/N-1` → … → 200 at 0.
pub fn redirect_chain_target(n: u32) -> Redirect {
    if n == 0 {
        Redirect::Terminal
    } else {
        Redirect::To(format!("/redirect-chain/{}", n - 1))
    }
}

/// An infinite loop of `size` steps. A crawler without loop detection
/// will follow this forever.
pub fn redirect_loop_target(step: u32, size: u32) -> String {
    let next = (step + 1) % size.max(1);
    format!("/redirect-loop/{size}/{next}")
}

pub fn clamp_hops(n: u32) -> u32 {
    n.min(MAX_HOPS)
}

pub fn clamp_delay_ms(ms: u64) -> u64 {
    ms.min(MAX_DELAY_MS)
}

/// Markup with unclosed tags, a stray `<`, and no closing `</html>`.
/// A correct parser still recovers both links.
pub fn malformed_html() -> String {
    "<!DOCTYPE html>\n<html><head><title>Malformed</title>\n\
     <body>\n<p>unclosed paragraph\n\
     <a href=\"/malformed-target-1\">one</a>\n\
     <div><span>nested but never closed\n\
     <a href=\"/malformed-target-2\">two</a>\n\
     <p>a stray < character and an <unknown-tag attr=unquoted>\n"
        .to_string()
}

/// A page of approximately `mb` megabytes, clamped to `MAX_HUGE_MB`.
pub fn huge_html(mb: usize) -> String {
    let mb = mb.clamp(1, MAX_HUGE_MB);
    let target = mb * 1024 * 1024;
    let head = "<!DOCTYPE html>\n<html><head><title>Huge</title></head><body>\n\
                <a href=\"/huge-target\">link</a>\n<p>";
    let tail = "</p>\n</body>\n</html>\n";
    let filler = "Large body content used to test streaming parsers and memory ceilings. ";

    let mut s = String::with_capacity(target + 256);
    s.push_str(head);
    while s.len() + tail.len() < target {
        s.push_str(filler);
    }
    s.push_str(tail);
    s
}
```

- [ ] **Step 4: Register the module**

Add to `crates/pounce-bench/src/lib.rs`:

```rust
pub mod pathological;
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p pounce-bench pathological`
Expected: PASS, 5 tests.

- [ ] **Step 6: Commit**

```bash
git add crates/pounce-bench/src/pathological.rs crates/pounce-bench/src/lib.rs
git commit -m "feat(bench): add pathological redirect, size, and malformed-markup cases"
```

---

## Task 6: The axum server

**Files:**
- Create: `crates/pounce-bench/src/server.rs`
- Modify: `crates/pounce-bench/src/lib.rs`
- Test: `crates/pounce-bench/tests/server_integration.rs`

Routing uses a `fallback` handler with a `HashMap` lookup rather than registering 100k individual routes — registering routes per page would make server startup O(n) in router construction and blow up memory.

**Note on axum 0.8:** path parameters use brace syntax (`/{param}`), not the colon syntax from 0.7. Using `:param` will panic at startup.

- [ ] **Step 1: Write the failing tests**

`crates/pounce-bench/tests/server_integration.rs`:

```rust
use pounce_bench::graph::{GraphSpec, SiteGraph};
use pounce_bench::server::{serve, Fixture};
use std::sync::Arc;

async fn spawn() -> String {
    let graph = SiteGraph::generate(&GraphSpec { seed: 5, page_count: 200, ..GraphSpec::default() });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base = format!("http://{addr}");
    let fixture = Arc::new(Fixture { graph, base_url: base.clone() });
    tokio::spawn(async move { serve(listener, fixture).await.unwrap() });
    base
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

#[tokio::test]
async fn serves_the_root_page() {
    let base = spawn().await;
    let res = client().get(&base).send().await.unwrap();
    assert_eq!(res.status(), 200);
    assert!(res.headers()["content-type"].to_str().unwrap().starts_with("text/html"));
    assert!(res.text().await.unwrap().contains("<h1>Home</h1>"));
}

#[tokio::test]
async fn unknown_paths_return_404() {
    let base = spawn().await;
    let res = client().get(format!("{base}/no-such-page")).send().await.unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn serves_robots_txt_disallowing_the_private_area() {
    let base = spawn().await;
    let res = client().get(format!("{base}/robots.txt")).send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body = res.text().await.unwrap();
    assert!(body.contains("User-agent: *"));
    assert!(body.contains("Disallow: /private/"));
    assert!(body.contains("Sitemap:"));
}

#[tokio::test]
async fn serves_a_sitemap_listing_indexable_pages() {
    let base = spawn().await;
    let body = client().get(format!("{base}/sitemap.xml")).send().await.unwrap().text().await.unwrap();
    assert!(body.starts_with("<?xml"));
    assert!(body.contains("<urlset"));
    assert!(body.contains("<loc>"));
    assert!(!body.contains("/private/"));
}

#[tokio::test]
async fn redirect_chain_terminates_in_a_200() {
    let base = spawn().await;
    let c = client();
    let res = c.get(format!("{base}/redirect-chain/3")).send().await.unwrap();
    assert_eq!(res.status(), 301);
    assert_eq!(res.headers()["location"], "/redirect-chain/2");

    let end = c.get(format!("{base}/redirect-chain/0")).send().await.unwrap();
    assert_eq!(end.status(), 200);
}

#[tokio::test]
async fn redirect_loop_never_terminates() {
    let base = spawn().await;
    let res = client().get(format!("{base}/redirect-loop/3/0")).send().await.unwrap();
    assert_eq!(res.status(), 302);
    assert_eq!(res.headers()["location"], "/redirect-loop/3/1");
}

#[tokio::test]
async fn status_endpoint_returns_the_requested_code() {
    let base = spawn().await;
    let c = client();
    for code in [404u16, 410, 500, 503] {
        let res = c.get(format!("{base}/status/{code}")).send().await.unwrap();
        assert_eq!(res.status().as_u16(), code);
    }
}

#[tokio::test]
async fn slow_endpoint_actually_delays() {
    let base = spawn().await;
    let start = std::time::Instant::now();
    let res = client().get(format!("{base}/slow/300")).send().await.unwrap();
    assert_eq!(res.status(), 200);
    assert!(start.elapsed().as_millis() >= 300);
}

#[tokio::test]
async fn malformed_endpoint_serves_broken_markup() {
    let base = spawn().await;
    let body = client().get(format!("{base}/malformed")).send().await.unwrap().text().await.unwrap();
    assert!(body.contains("/malformed-target-1"));
    assert!(!body.contains("</html>"));
}

#[tokio::test]
async fn every_generated_page_is_actually_reachable_over_http() {
    let base = spawn().await;
    let graph = SiteGraph::generate(&GraphSpec { seed: 5, page_count: 200, ..GraphSpec::default() });
    let c = client();
    for node in graph.nodes.iter().take(25) {
        let res = c.get(format!("{base}{}", node.path)).send().await.unwrap();
        assert_eq!(res.status(), 200, "path {} was not served", node.path);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p pounce-bench --test server_integration`
Expected: FAIL — `unresolved import pounce_bench::server`.

- [ ] **Step 3: Write the implementation**

`crates/pounce-bench/src/server.rs`:

```rust
//! The fixture HTTP server.
//!
//! Pages are resolved through a hash lookup in a fallback handler rather
//! than registered as individual routes — a 100k-route router would be
//! slow to build and large to hold.

use crate::graph::SiteGraph;
use crate::pathological::{
    clamp_delay_ms, clamp_hops, huge_html, malformed_html, redirect_chain_target,
    redirect_loop_target, Redirect,
};
use crate::render::render_page;
use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use std::sync::Arc;
use std::time::Duration;

pub struct Fixture {
    pub graph: SiteGraph,
    pub base_url: String,
}

type Shared = Arc<Fixture>;

pub fn app(fixture: Shared) -> Router {
    Router::new()
        .route("/robots.txt", get(robots))
        .route("/sitemap.xml", get(sitemap))
        .route("/malformed", get(malformed))
        .route("/slow/{ms}", get(slow))
        .route("/huge/{mb}", get(huge))
        .route("/status/{code}", get(status))
        .route("/redirect-chain/{n}", get(chain))
        .route("/redirect-loop/{size}/{step}", get(loop_))
        .fallback(page)
        .with_state(fixture)
}

pub async fn serve(listener: tokio::net::TcpListener, fixture: Shared) -> anyhow::Result<()> {
    axum::serve(listener, app(fixture)).await?;
    Ok(())
}

fn html(body: String) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "text/html; charset=utf-8".parse().unwrap());
    (StatusCode::OK, headers, body).into_response()
}

async fn page(State(fx): State<Shared>, uri: axum::http::Uri) -> Response {
    match fx.graph.lookup(uri.path()) {
        Some(id) => html(render_page(&fx.graph, id, &fx.base_url)),
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

async fn robots(State(fx): State<Shared>) -> Response {
    let body = format!(
        "User-agent: *\nDisallow: /private/\nAllow: /\n\nSitemap: {}/sitemap.xml\n",
        fx.base_url
    );
    (StatusCode::OK, [(header::CONTENT_TYPE, "text/plain; charset=utf-8")], body).into_response()
}

async fn sitemap(State(fx): State<Shared>) -> Response {
    let mut s = String::with_capacity(fx.graph.nodes.len() * 90);
    s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    s.push_str("<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n");
    for node in fx.graph.nodes.iter().filter(|n| !n.noindex) {
        s.push_str("  <url><loc>");
        s.push_str(&fx.base_url);
        s.push_str(&node.path);
        s.push_str("</loc></url>\n");
    }
    s.push_str("</urlset>\n");
    (StatusCode::OK, [(header::CONTENT_TYPE, "application/xml")], s).into_response()
}

async fn malformed() -> Response {
    html(malformed_html())
}

async fn slow(Path(ms): Path<u64>) -> Response {
    tokio::time::sleep(Duration::from_millis(clamp_delay_ms(ms))).await;
    html("<!DOCTYPE html><html><head><title>Slow</title></head><body><h1>Slow</h1></body></html>".into())
}

async fn huge(Path(mb): Path<usize>) -> Response {
    html(huge_html(mb))
}

async fn status(Path(code): Path<u16>) -> Response {
    let status = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    (status, format!("status {code}")).into_response()
}

async fn chain(Path(n): Path<u32>) -> Response {
    match redirect_chain_target(clamp_hops(n)) {
        Redirect::Terminal => {
            html("<!DOCTYPE html><html><head><title>Chain end</title></head><body><h1>Chain end</h1></body></html>".into())
        }
        Redirect::To(next) => {
            (StatusCode::MOVED_PERMANENTLY, [(header::LOCATION, next)]).into_response()
        }
    }
}

async fn loop_(Path((size, step)): Path<(u32, u32)>) -> Response {
    let next = redirect_loop_target(step, clamp_hops(size).max(2));
    (StatusCode::FOUND, [(header::LOCATION, next)]).into_response()
}
```

- [ ] **Step 4: Register the module**

Add to `crates/pounce-bench/src/lib.rs`:

```rust
pub mod server;
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p pounce-bench --test server_integration`
Expected: PASS, 10 tests.

If startup panics with `Path segments must not start with :`, a route still uses axum 0.7 colon syntax — convert it to `{param}`.

- [ ] **Step 6: Commit**

```bash
git add crates/pounce-bench/src/server.rs crates/pounce-bench/src/lib.rs crates/pounce-bench/tests/server_integration.rs
git commit -m "feat(bench): serve the fixture site over HTTP with pathological routes"
```

---

## Task 7: The fixture-site binary

**Files:**
- Create: `crates/pounce-bench/src/bin/fixture_site.rs`

- [ ] **Step 1: Write the binary**

```rust
//! Standalone fixture site server.
//!
//! Usage: fixture-site --pages 100000 --seed 42 --port 8080

use clap::Parser;
use pounce_bench::graph::{GraphSpec, SiteGraph};
use pounce_bench::server::{serve, Fixture};
use std::sync::Arc;
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(name = "fixture-site", about = "Deterministic fixture website for benchmarking")]
struct Args {
    #[arg(long, default_value_t = 100_000)]
    pages: u32,
    #[arg(long, default_value_t = 42)]
    seed: u64,
    #[arg(long, default_value_t = 8080)]
    port: u16,
    #[arg(long, default_value_t = 6)]
    max_depth: u16,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let start = Instant::now();
    let graph = SiteGraph::generate(&GraphSpec {
        seed: args.seed,
        page_count: args.pages,
        max_depth: args.max_depth,
        ..GraphSpec::default()
    });
    let total_links: usize = graph.nodes.iter().map(|n| n.outlinks.len()).sum();
    eprintln!(
        "generated {} pages, {} links, mean {:.1} links/page in {:?}",
        graph.nodes.len(),
        total_links,
        total_links as f64 / graph.nodes.len() as f64,
        start.elapsed()
    );

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", args.port)).await?;
    let addr = listener.local_addr()?;
    let base_url = format!("http://{addr}");
    eprintln!("fixture site ready at {base_url} (seed {})", args.seed);
    // stdout carries only the URL, so a runner can capture it cleanly.
    println!("{base_url}");

    serve(listener, Arc::new(Fixture { graph, base_url })).await
}
```

- [ ] **Step 2: Build and run it**

Run: `cargo run --release -p pounce-bench --bin fixture-site -- --pages 100000 --seed 42 --port 8080`
Expected: stderr reports generation stats and readiness; stdout prints `http://127.0.0.1:8080`. Generation of 100k pages should complete in under ~2 seconds.

- [ ] **Step 3: Verify by hand**

In a second terminal:

```bash
curl -s http://127.0.0.1:8080/ | head -20
curl -s http://127.0.0.1:8080/robots.txt
curl -sI http://127.0.0.1:8080/redirect-chain/3
curl -s http://127.0.0.1:8080/sitemap.xml | head -5
```

Expected: an HTML document with a nav and links, a robots.txt with a Sitemap line, a `301` with a `Location` header, and an XML sitemap.

- [ ] **Step 4: Commit**

```bash
git add crates/pounce-bench/src/bin/fixture_site.rs
git commit -m "feat(bench): add fixture-site binary"
```

---

## Task 8: Process metrics sampler

**Files:**
- Create: `crates/pounce-bench/src/metrics.rs`
- Modify: `crates/pounce-bench/src/lib.rs`

This module knows nothing about crawling. It runs a command and watches its memory, which is what makes it usable against competitors' binaries as well as our own.

- [ ] **Step 1: Write the failing tests**

Create `crates/pounce-bench/src/metrics.rs` containing only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn sleep_cmd(secs: u32) -> Vec<String> {
        if cfg!(windows) {
            vec![
                "powershell".into(), "-NoProfile".into(), "-Command".into(),
                format!("Start-Sleep -Seconds {secs}"),
            ]
        } else {
            vec!["sh".into(), "-c".into(), format!("sleep {secs}")]
        }
    }

    #[test]
    fn measures_wall_time_of_a_successful_command() {
        let m = run_measured(&sleep_cmd(1), None).unwrap();
        assert_eq!(m.exit_code, 0);
        assert!(m.wall_ms >= 900, "wall_ms was {}", m.wall_ms);
        assert!(m.wall_ms < 10_000);
    }

    #[test]
    fn records_nonzero_peak_memory() {
        let m = run_measured(&sleep_cmd(1), None).unwrap();
        assert!(m.peak_rss_bytes > 0, "sampler never observed the process");
    }

    #[test]
    fn propagates_a_failing_exit_code() {
        let cmd: Vec<String> = if cfg!(windows) {
            vec!["cmd".into(), "/C".into(), "exit 3".into()]
        } else {
            vec!["sh".into(), "-c".into(), "exit 3".into()]
        };
        let m = run_measured(&cmd, None).unwrap();
        assert_eq!(m.exit_code, 3);
    }

    #[test]
    fn errors_on_an_empty_command() {
        assert!(run_measured(&[], None).is_err());
    }

    #[test]
    fn kills_a_command_that_exceeds_its_timeout() {
        let m = run_measured(&sleep_cmd(60), Some(std::time::Duration::from_secs(2))).unwrap();
        assert!(m.timed_out);
        assert!(m.wall_ms < 20_000);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p pounce-bench metrics`
Expected: FAIL — `cannot find function run_measured`.

- [ ] **Step 3: Write the implementation**

Insert above the `#[cfg(test)]` module in `crates/pounce-bench/src/metrics.rs`:

```rust
//! Spawns a child process and samples its resident memory while it runs.
//!
//! Deliberately agnostic about what it is running, so the same code
//! measures Pounce and every competitor identically — which is the only
//! way the resulting comparison means anything.

use anyhow::{bail, Context, Result};
use std::process::Command;
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

/// How often to sample the child's memory. 50ms is frequent enough to
/// catch a peak without meaningfully perturbing the measurement.
const SAMPLE_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug, Clone)]
pub struct Measurement {
    pub wall_ms: u128,
    pub peak_rss_bytes: u64,
    pub exit_code: i32,
    pub timed_out: bool,
}

/// Runs `cmd` to completion, sampling memory throughout.
///
/// `cmd[0]` is the program, the rest are arguments. If `timeout` elapses
/// the child is killed and `timed_out` is set.
pub fn run_measured(cmd: &[String], timeout: Option<Duration>) -> Result<Measurement> {
    let Some((program, args)) = cmd.split_first() else {
        bail!("command must not be empty");
    };

    let start = Instant::now();
    let mut child = Command::new(program)
        .args(args)
        .spawn()
        .with_context(|| format!("failed to spawn {program}"))?;

    let pid = Pid::from_u32(child.id());
    let mut sys = System::new();
    let mut peak_rss_bytes = 0u64;
    let mut timed_out = false;

    let exit_code = loop {
        match child.try_wait().context("failed to poll child")? {
            Some(status) => break status.code().unwrap_or(-1),
            None => {
                sys.refresh_processes_specifics(
                    ProcessesToUpdate::Some(&[pid]),
                    true,
                    ProcessRefreshKind::nothing().with_memory(),
                );
                if let Some(proc) = sys.process(pid) {
                    // sysinfo reports memory in bytes.
                    peak_rss_bytes = peak_rss_bytes.max(proc.memory());
                }

                if let Some(limit) = timeout {
                    if start.elapsed() >= limit {
                        timed_out = true;
                        let _ = child.kill();
                        let status = child.wait().context("failed to reap killed child")?;
                        break status.code().unwrap_or(-1);
                    }
                }

                std::thread::sleep(SAMPLE_INTERVAL);
            }
        }
    };

    Ok(Measurement {
        wall_ms: start.elapsed().as_millis(),
        peak_rss_bytes,
        exit_code,
        timed_out,
    })
}
```

- [ ] **Step 4: Register the module**

Add to `crates/pounce-bench/src/lib.rs`:

```rust
pub mod metrics;
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p pounce-bench metrics -- --test-threads=1`
Expected: PASS, 5 tests, taking roughly 5 seconds total.

`records_nonzero_peak_memory` is the one that can legitimately fail: if the child exits faster than the first sample, peak stays 0. That is why the test sleeps for a full second. If it still fails, verify the sysinfo API — `refresh_processes_specifics`'s signature has changed across versions, and the middle `bool` argument (remove dead processes) was added in 0.33.

- [ ] **Step 6: Commit**

```bash
git add crates/pounce-bench/src/metrics.rs crates/pounce-bench/src/lib.rs
git commit -m "feat(bench): sample child-process wall time and peak RSS"
```

---

## Task 9: Report types and formatting

**Files:**
- Create: `crates/pounce-bench/src/report.rs`
- Modify: `crates/pounce-bench/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

Create `crates/pounce-bench/src/report.rs` containing only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Report {
        Report {
            fixture_seed: 42,
            fixture_pages: 100_000,
            results: vec![
                BenchResult {
                    tool: "pounce".into(),
                    wall_ms: 40_000,
                    peak_rss_bytes: 300 * 1024 * 1024,
                    pages_crawled: 100_000,
                    exit_code: 0,
                    timed_out: false,
                },
                BenchResult {
                    tool: "freecrawl".into(),
                    wall_ms: 120_000,
                    peak_rss_bytes: 1500 * 1024 * 1024,
                    pages_crawled: 100_000,
                    exit_code: 0,
                    timed_out: false,
                },
            ],
        }
    }

    #[test]
    fn computes_throughput() {
        assert_eq!(sample().results[0].urls_per_sec(), 2500.0);
    }

    #[test]
    fn throughput_is_zero_when_no_time_elapsed() {
        let r = BenchResult { wall_ms: 0, ..sample().results[0].clone() };
        assert_eq!(r.urls_per_sec(), 0.0);
    }

    #[test]
    fn renders_a_markdown_table_with_a_row_per_tool() {
        let md = sample().to_markdown();
        assert!(md.contains("| Tool |"));
        assert!(md.contains("| pounce |"));
        assert!(md.contains("| freecrawl |"));
        assert!(md.contains("2500"));
        assert!(md.contains("seed 42"));
    }

    #[test]
    fn marks_timed_out_runs_in_the_table() {
        let mut r = sample();
        r.results[1].timed_out = true;
        assert!(r.to_markdown().contains("timed out"));
    }

    #[test]
    fn serialises_to_json_round_trip() {
        let json = serde_json::to_string(&sample()).unwrap();
        let back: Report = serde_json::from_str(&json).unwrap();
        assert_eq!(back.results.len(), 2);
        assert_eq!(back.fixture_seed, 42);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p pounce-bench report`
Expected: FAIL — `cannot find type Report`.

- [ ] **Step 3: Write the implementation**

Insert above the `#[cfg(test)]` module in `crates/pounce-bench/src/report.rs`:

```rust
//! Benchmark results and their published representations.
//!
//! The markdown table produced here is the project's primary marketing
//! asset, so it must be honest: timed-out and partial runs are labelled
//! rather than omitted.

use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchResult {
    pub tool: String,
    pub wall_ms: u128,
    pub peak_rss_bytes: u64,
    pub pages_crawled: u64,
    pub exit_code: i32,
    pub timed_out: bool,
}

impl BenchResult {
    pub fn urls_per_sec(&self) -> f64 {
        if self.wall_ms == 0 {
            return 0.0;
        }
        self.pages_crawled as f64 / (self.wall_ms as f64 / 1000.0)
    }

    pub fn peak_rss_mb(&self) -> f64 {
        self.peak_rss_bytes as f64 / (1024.0 * 1024.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub fixture_seed: u64,
    pub fixture_pages: u32,
    pub results: Vec<BenchResult>,
}

impl Report {
    pub fn to_markdown(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(
            s,
            "Fixture: {} pages, seed {}.\n",
            self.fixture_pages, self.fixture_seed
        );
        s.push_str("| Tool | URLs/sec | Peak RSS (MB) | Wall (s) | Pages | Status |\n");
        s.push_str("|---|---:|---:|---:|---:|---|\n");
        for r in &self.results {
            let status = if r.timed_out {
                "timed out".to_string()
            } else if r.exit_code != 0 {
                format!("exit {}", r.exit_code)
            } else {
                "ok".to_string()
            };
            let _ = writeln!(
                s,
                "| {} | {:.0} | {:.0} | {:.1} | {} | {} |",
                r.tool,
                r.urls_per_sec(),
                r.peak_rss_mb(),
                r.wall_ms as f64 / 1000.0,
                r.pages_crawled,
                status
            );
        }
        s
    }
}
```

- [ ] **Step 4: Register the module**

Add to `crates/pounce-bench/src/lib.rs`:

```rust
pub mod report;
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p pounce-bench report`
Expected: PASS, 5 tests.

- [ ] **Step 6: Commit**

```bash
git add crates/pounce-bench/src/report.rs crates/pounce-bench/src/lib.rs
git commit -m "feat(bench): add benchmark result types with markdown and JSON output"
```

---

## Task 10: The benchmark runner binary

**Files:**
- Create: `crates/pounce-bench/src/bin/bench_runner.rs`

The runner starts the fixture site in-process, then runs each configured tool against it with an identical command template.

- [ ] **Step 1: Write the binary**

```rust
//! Runs one or more crawlers against an in-process fixture site and
//! reports throughput and peak memory for each.
//!
//! Usage:
//!   bench-runner --pages 100000 \
//!     --tool 'pounce=./target/release/pounce crawl {url} --quiet' \
//!     --tool 'freecrawl=freecrawl crawl {url}' \
//!     --out bench-results/run.json

use anyhow::{bail, Context, Result};
use clap::Parser;
use pounce_bench::graph::{GraphSpec, SiteGraph};
use pounce_bench::metrics::run_measured;
use pounce_bench::report::{BenchResult, Report};
use pounce_bench::server::{serve, Fixture};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(name = "bench-runner", about = "Benchmark crawlers against a deterministic fixture site")]
struct Args {
    #[arg(long, default_value_t = 100_000)]
    pages: u32,
    #[arg(long, default_value_t = 42)]
    seed: u64,
    /// A tool to benchmark, as `name=command`. `{url}` is replaced with
    /// the fixture site's base URL. Repeatable.
    #[arg(long = "tool", value_name = "NAME=COMMAND")]
    tools: Vec<String>,
    /// Per-tool timeout in seconds.
    #[arg(long, default_value_t = 1800)]
    timeout_secs: u64,
    /// Where to write the JSON report.
    #[arg(long, default_value = "bench-results/run.json")]
    out: PathBuf,
}

fn parse_tool(spec: &str) -> Result<(String, String)> {
    let Some((name, command)) = spec.split_once('=') else {
        bail!("tool spec must be NAME=COMMAND, got: {spec}");
    };
    if name.is_empty() || command.trim().is_empty() {
        bail!("tool spec has an empty name or command: {spec}");
    }
    Ok((name.to_string(), command.to_string()))
}

/// Splits a command string on whitespace. Deliberately simple — quoted
/// arguments with spaces are not supported, and a tool needing them
/// should be wrapped in a shell script.
fn split_command(command: &str, url: &str) -> Vec<String> {
    command
        .split_whitespace()
        .map(|part| part.replace("{url}", url))
        .collect()
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if args.tools.is_empty() {
        bail!("at least one --tool is required");
    }
    let tools: Vec<(String, String)> =
        args.tools.iter().map(|s| parse_tool(s)).collect::<Result<_>>()?;

    // Stand up the fixture site on an ephemeral port.
    let graph = SiteGraph::generate(&GraphSpec {
        seed: args.seed,
        page_count: args.pages,
        ..GraphSpec::default()
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let base_url = format!("http://{}", listener.local_addr()?);
    eprintln!("fixture site: {base_url} ({} pages, seed {})", args.pages, args.seed);

    let fixture = Arc::new(Fixture { graph, base_url: base_url.clone() });
    let server = tokio::spawn(async move { serve(listener, fixture).await });

    let mut results = Vec::new();
    for (name, command) in &tools {
        let argv = split_command(command, &base_url);
        eprintln!("running {name}: {}", argv.join(" "));

        // Measurement is blocking; keep it off the async runtime.
        let argv2 = argv.clone();
        let timeout = Duration::from_secs(args.timeout_secs);
        let measurement =
            tokio::task::spawn_blocking(move || run_measured(&argv2, Some(timeout))).await??;

        eprintln!(
            "  {name}: {:.1}s, peak {:.0} MB, exit {}{}",
            measurement.wall_ms as f64 / 1000.0,
            measurement.peak_rss_bytes as f64 / (1024.0 * 1024.0),
            measurement.exit_code,
            if measurement.timed_out { " (TIMED OUT)" } else { "" }
        );

        results.push(BenchResult {
            tool: name.clone(),
            wall_ms: measurement.wall_ms,
            peak_rss_bytes: measurement.peak_rss_bytes,
            // Assumes the tool crawled the whole site. Once pounce-cli
            // emits a JSON summary, read the real count from it instead.
            pages_crawled: u64::from(args.pages),
            exit_code: measurement.exit_code,
            timed_out: measurement.timed_out,
        });
    }

    server.abort();

    let report = Report { fixture_seed: args.seed, fixture_pages: args.pages, results };

    if let Some(dir) = args.out.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("failed to create {}", dir.display()))?;
    }
    std::fs::write(&args.out, serde_json::to_string_pretty(&report)?)
        .with_context(|| format!("failed to write {}", args.out.display()))?;

    println!("\n{}", report.to_markdown());
    eprintln!("wrote {}", args.out.display());
    Ok(())
}
```

- [ ] **Step 2: Build it**

Run: `cargo build --release -p pounce-bench`
Expected: builds clean.

- [ ] **Step 3: Smoke-test with a stand-in crawler**

There is no crawler yet, so measure `curl` fetching the root page — this proves the harness end to end.

On a POSIX shell:

```bash
cargo run --release -p pounce-bench --bin bench-runner -- \
  --pages 1000 \
  --tool 'curl=curl -s -o /dev/null {url}' \
  --out bench-results/smoke.json
```

Expected: a markdown table printed to stdout with one `curl` row, exit status `ok`, and `bench-results/smoke.json` written.

- [ ] **Step 4: Verify the JSON**

Run: `cat bench-results/smoke.json`
Expected: valid JSON with `fixture_seed`, `fixture_pages`, and a one-element `results` array.

- [ ] **Step 5: Commit**

```bash
git add crates/pounce-bench/src/bin/bench_runner.rs
git commit -m "feat(bench): add bench-runner comparing crawlers on a shared fixture"
```

---

## Task 11: Criterion parse benchmarks

This measures the parsing hot path in isolation — the single biggest claimed advantage over Node and Python competitors, and the number most likely to regress silently.

**Files:**
- Create: `crates/pounce-bench/benches/parse.rs`

- [ ] **Step 1: Write the benchmark**

```rust
//! Parse-throughput benchmarks.
//!
//! Measures link and metadata extraction over fixture pages, which is
//! the hot path in any crawl. Throughput is reported in bytes/sec so
//! results stay comparable as fixture page sizes change.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use lol_html::{element, HtmlRewriter, Settings};
use pounce_bench::graph::{GraphSpec, SiteGraph};
use pounce_bench::render::render_page;
use std::hint::black_box;

fn corpus(pages: u32) -> Vec<String> {
    let graph = SiteGraph::generate(&GraphSpec { seed: 42, page_count: pages, ..GraphSpec::default() });
    (0..pages).map(|id| render_page(&graph, id, "http://localhost:8080")).collect()
}

/// Extracts what a crawler actually needs: links, title, canonical, and
/// image alt attributes — in a single streaming pass, no DOM built.
fn extract(html: &str) -> usize {
    let mut found = 0usize;
    let mut rewriter = HtmlRewriter::new(
        Settings {
            element_content_handlers: vec![
                element!("a[href]", |el| {
                    if el.get_attribute("href").is_some() {
                        found += 1;
                    }
                    Ok(())
                }),
                element!("link[rel=canonical]", |el| {
                    if el.get_attribute("href").is_some() {
                        found += 1;
                    }
                    Ok(())
                }),
                element!("meta[name=description]", |el| {
                    if el.get_attribute("content").is_some() {
                        found += 1;
                    }
                    Ok(())
                }),
                element!("img", |el| {
                    if el.get_attribute("alt").is_none() {
                        found += 1;
                    }
                    Ok(())
                }),
            ],
            ..Settings::new()
        },
        |_: &[u8]| {},
    );
    rewriter.write(html.as_bytes()).unwrap();
    rewriter.end().unwrap();
    found
}

fn bench_extract(c: &mut Criterion) {
    let pages = corpus(200);
    let total_bytes: usize = pages.iter().map(|p| p.len()).sum();
    let mean_kb = total_bytes as f64 / pages.len() as f64 / 1024.0;

    let mut group = c.benchmark_group("extract");
    group.throughput(Throughput::Bytes(total_bytes as u64));
    group.bench_function(BenchmarkId::new("lol_html", format!("{mean_kb:.0}KB_pages")), |b| {
        b.iter(|| {
            let mut total = 0usize;
            for page in &pages {
                total += extract(black_box(page));
            }
            black_box(total)
        })
    });
    group.finish();
}

fn bench_render(c: &mut Criterion) {
    // Fixture generation speed matters too — a slow generator makes the
    // whole harness annoying to run.
    let graph = SiteGraph::generate(&GraphSpec { seed: 42, page_count: 1000, ..GraphSpec::default() });
    c.bench_function("render_page", |b| {
        b.iter(|| black_box(render_page(&graph, black_box(500), "http://localhost:8080")))
    });
}

criterion_group!(benches, bench_extract, bench_render);
criterion_main!(benches);
```

- [ ] **Step 2: Run the benchmarks**

Run: `cargo bench -p pounce-bench`
Expected: criterion reports a throughput figure in MiB/s for `extract/lol_html` and a per-call time for `render_page`. Record the extract throughput — it is the baseline every future change is measured against.

- [ ] **Step 3: Commit**

```bash
git add crates/pounce-bench/benches/parse.rs
git commit -m "feat(bench): add criterion parse and render throughput benchmarks"
```

---

## Task 12: CI

**Files:**
- Create: `.github/workflows/ci.yml`

- [ ] **Step 1: Write the workflow**

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

jobs:
  check:
    name: fmt + clippy
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all -- --check
      - run: cargo clippy --workspace --all-targets -- -D warnings

  test:
    name: test (${{ matrix.os }})
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --workspace --all-targets

  bench-smoke:
    name: benchmarks compile and run
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      # Proves the benches still compile and run without spending CI
      # minutes on statistically meaningful sample counts.
      - run: cargo bench -p pounce-bench -- --test
```

- [ ] **Step 2: Verify locally before pushing**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
```

Expected: all three clean. Clippy will likely flag `needless_range_loop` in `render.rs`'s image loop — either restructure or add a targeted `#[allow]` with a comment explaining why the index is needed.

- [ ] **Step 3: Commit and push**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add fmt, clippy, cross-platform test, and bench smoke jobs"
git push -u origin main
```

- [ ] **Step 4: Confirm CI is green**

Check the Actions tab on GitHub. All three jobs must pass on all three platforms before Phase 0 is considered done.

---

## Task 13: Document the harness

**Files:**
- Create: `crates/pounce-bench/README.md`
- Modify: `docs/specs/2026-08-19-pounce-design.md`

- [ ] **Step 1: Write the crate README**

`crates/pounce-bench/README.md`:

````markdown
# pounce-bench

A deterministic fixture website and a benchmark runner. Everything Pounce
claims about speed is measured here.

## Why a fixture site

Benchmarking against a real website measures the network, not the crawler,
and the result is unreproducible the moment that site changes. The fixture
site is generated from a seed, so `--seed 42 --pages 100000` produces a
byte-identical site on any machine, forever.

## Run the fixture site

```bash
cargo run --release -p pounce-bench --bin fixture-site -- --pages 100000 --seed 42 --port 8080
```

Routes beyond the generated pages:

| Route | Behaviour |
|---|---|
| `/robots.txt` | Disallows `/private/`, points at the sitemap |
| `/sitemap.xml` | Lists every indexable page |
| `/redirect-chain/{n}` | `n` chained 301s, terminating in a 200 |
| `/redirect-loop/{size}/{step}` | An infinite 302 loop of `size` steps |
| `/status/{code}` | Returns the requested status code |
| `/slow/{ms}` | Delays `ms` milliseconds, capped at 30s |
| `/huge/{mb}` | A page of `mb` megabytes, capped at 16 |
| `/malformed` | Unclosed tags, stray `<`, no `</html>` |

## Compare crawlers

```bash
cargo run --release -p pounce-bench --bin bench-runner -- \
  --pages 100000 \
  --tool 'pounce=./target/release/pounce crawl {url} --quiet' \
  --tool 'freecrawl=freecrawl crawl {url}' \
  --out bench-results/run.json
```

`{url}` is replaced with the fixture site's base URL. Every tool gets the
same site, the same machine, and the same measurement code.

## Parse benchmarks

```bash
cargo bench -p pounce-bench
```

## Reporting rules

Publish timed-out and failed runs rather than dropping them. A benchmark
that quietly omits the cases where a competitor struggles is not a
benchmark, it is an advertisement — and it will be found out.
````

- [ ] **Step 2: Mark Phase 0 complete in the spec**

In `docs/specs/2026-08-19-pounce-design.md`, in the §7 Roadmap table, change the **0.0** row's Duration cell from `2 wks` to `2 wks ✅ complete`.

- [ ] **Step 3: Commit**

```bash
git add crates/pounce-bench/README.md docs/specs/2026-08-19-pounce-design.md
git commit -m "docs(bench): document the fixture site and benchmark runner"
```

---

## Phase 0 exit criteria

Phase 0 is done when all of these hold:

- [ ] `cargo test --workspace` passes on Linux, macOS, and Windows in CI
- [ ] `fixture-site --pages 100000` generates and starts serving in under 5 seconds
- [ ] `bench-runner` produces a markdown table and a JSON report for at least one real crawler
- [ ] `cargo bench -p pounce-bench` reports a parse throughput baseline, recorded in the commit message
- [ ] At least one competitor (FreeCrawl or Screaming Frog in CLI mode) has been benchmarked against a 100k-page fixture, and the numbers are written down

**Then make the call.** If a competitor's throughput on a 100k-page fixture is within roughly 20% of what a Rust crawler could plausibly achieve, the core premise is weak and the positioning needs to change before any more effort goes in. That decision is the entire point of Phase 0, and it should be made on the numbers rather than on enthusiasm.

---

## Deferred to Phase 1

Deliberately not built here, to keep Phase 0 to two weeks:

- Any actual crawler code
- JS-rendered fixture pages (needs a headless browser; the crawler cannot use them yet)
- Multi-host fixtures for testing cross-domain link classification
- HTTP/2 and compression variants of the fixture
- Historical benchmark tracking and charts
- Bench regression thresholds in CI (needs a stable baseline first, which needs a crawler)
