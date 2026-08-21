# M2 Audit Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the machinery that runs audit rules during and after a crawl and stores what they find — with zero rules shipped, so the 30 rules can land afterwards in independently reviewable batches.

**Architecture:** Two traits, one registry, per the approved design in [`docs/specs/2026-08-21-audit-rule-engine.md`](../specs/2026-08-21-audit-rule-engine.md). `PageRule` sees one `PageRecord` and nothing else, so it cannot issue a query and cannot blow the 10% wall-time budget. `SiteRule` sees the finished database and runs indexed aggregates. Issues commit in the same transaction as their page.

**Tech Stack:** Rust 2024, `rusqlite` (bundled SQLite), `lol_html`, `criterion`. No new third-party dependencies in this plan.

## Global Constraints

- **No new third-party dependencies.** Every crate needed is already a workspace dependency. If a task seems to need one, stop and raise it.
- **TDD, strictly.** Write the failing test, run it, confirm it fails *for the right reason* (a stub that panics — a compile error is not red), then implement. Vacuous assertions have shipped in this repo before.
- **One commit per task.** Conventional commits (`feat(audit):`, `test(bench):`). Messages explain **why** — the tradeoff taken, the alternative rejected.
- **`PLAN.md` is updated in the same commit as the work**, ticking the task and recording any deviation inline.
- **Verification before every commit:** `cargo fmt --all` && `cargo clippy --workspace --all-targets -- -D warnings` && `cargo test --workspace --lib --bins --tests -- --test-threads=1`.
- **Never combine `--all-targets` with `-- <harness args>`** — criterion benches reject `--test-threads` and exit 2.
- **Absent is not empty.** `Option` carries that distinction into SQL as NULL vs `''`. Never collapse them.
- **Storage is disk-backed; memory stays flat.** No structure may grow proportional to crawl size.
- **`STRICT` tables only.** A status stored as text sorts as text.
- **Indices that no rule reads *during* a crawl are built in `Store::build_query_indices`, never in a migration.** This is what took 500k from 1,391 s to 143 s.

## File Structure

| File | Responsibility |
|---|---|
| `crates/pounce-bench/src/graph.rs` | *modify* — external links and nofollow marks on `PageNode` |
| `crates/pounce-bench/src/render.rs` | *modify* — emit them |
| `crates/pounce-parse/src/hash.rs` | **new** — stable FNV-1a for `body_hash` |
| `crates/pounce-parse/src/record.rs` | *modify* — `title_count`, `body_hash` |
| `crates/pounce-parse/src/extract.rs` | *modify* — populate both in the existing pass |
| `crates/pounce-audit/src/lib.rs` | **new crate** — re-exports |
| `crates/pounce-audit/src/issue.rs` | **new** — `Severity`, `RuleMeta`, `Issue` |
| `crates/pounce-audit/src/rule.rs` | **new** — `PageRule`, `SiteRule` |
| `crates/pounce-audit/src/registry.rs` | **new** — holds both, owns set-level invariants |
| `crates/pounce-store/src/migrations/008_issues.sql` | **new** — `issues` table |
| `crates/pounce-store/src/writer.rs` | *modify* — `push` returns the page id; `issues()` |
| `crates/pounce-cli/src/lib.rs` | *modify* — run page rules, then site rules |
| `crates/pounce-bench/benches/audit.rs` | **new** — the 10% budget measurement |

---

### Task 1: Fixture emits external links and `rel="nofollow"`

The fixture has neither, so `broken external link`, `orphan page` and any nofollow-sensitive rule cannot be tested end to end. This changes rendered page bytes, so **the baselines must be re-taken in this task** — doing it now means re-taking once rather than during batch four.

**Files:**
- Modify: `crates/pounce-bench/src/graph.rs`
- Modify: `crates/pounce-bench/src/render.rs`
- Test: `crates/pounce-bench/tests/graph_properties.rs`, `crates/pounce-bench/tests/render_output.rs`

**Interfaces:**
- Consumes: `SiteGraph::generate(&GraphSpec)`, `PageNode`, `render_page(&SiteGraph, u32, &str)`
- Produces: `GraphSpec { external_links: u32, .. }`, `PageNode { external: Vec<String>, nofollow_outlinks: u8, .. }`

- [ ] **Step 1: Write the failing tests**

In `crates/pounce-bench/tests/graph_properties.rs`:

```rust
#[test]
fn pages_carry_external_links() {
    let graph = SiteGraph::generate(&GraphSpec {
        seed: 42,
        page_count: 500,
        ..GraphSpec::default()
    });
    let total: usize = graph.nodes.iter().map(|n| n.external.len()).sum();
    assert!(total > 0, "no external links generated");
    // Every one must be absolute and off-site, or it is not an external link.
    for node in &graph.nodes {
        for url in &node.external {
            assert!(url.starts_with("http"), "{url}");
            assert!(!url.contains("127.0.0.1"), "{url}");
        }
    }
}

#[test]
fn external_links_are_deterministic() {
    let spec = GraphSpec { seed: 7, page_count: 200, ..GraphSpec::default() };
    let a = SiteGraph::generate(&spec);
    let b = SiteGraph::generate(&spec);
    let ext = |g: &SiteGraph| -> Vec<Vec<String>> {
        g.nodes.iter().map(|n| n.external.clone()).collect()
    };
    assert_eq!(ext(&a), ext(&b));
}

#[test]
fn some_pages_mark_outlinks_nofollow() {
    let graph = SiteGraph::generate(&GraphSpec {
        seed: 42,
        page_count: 500,
        ..GraphSpec::default()
    });
    let marked = graph.nodes.iter().filter(|n| n.nofollow_outlinks > 0).count();
    assert!(marked > 0, "no nofollow links generated");
    // Never more than the page actually has, or the renderer would index past
    // the end of outlinks.
    for node in &graph.nodes {
        assert!(node.nofollow_outlinks as usize <= node.outlinks.len());
    }
}
```

In `crates/pounce-bench/tests/render_output.rs`:

```rust
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
```

- [ ] **Step 2: Run the tests and confirm they fail for the right reason**

Run: `cargo test -p pounce-bench --test graph_properties --test render_output`
Expected: FAIL. `PageNode` has no field `external` / `nofollow_outlinks`.
This is a compile error, which is **not** valid red. Add the two fields to `PageNode` with `external: Vec::new()` and `nofollow_outlinks: 0` at every construction site, then re-run. Expected now: FAIL on the assertions (`no external links generated`).

- [ ] **Step 3: Generate them, deterministically**

In `crates/pounce-bench/src/graph.rs`, add to `GraphSpec`:

```rust
    /// External links added per page. Kept low: they exist so link-scope and
    /// broken-external-link rules have something to find, not to change the
    /// shape of the graph.
    pub external_links: u32,
```

with `external_links: 1` in `Default`. Then, inside node generation, using the existing `SplitMix64` — **never `rand`**, per the fixture-determinism invariant:

```rust
/// Hosts that will never resolve, so a crawler must report them as
/// unreachable rather than accidentally reaching something real.
const EXTERNAL_HOSTS: &[&str] = &[
    "https://example.invalid",
    "http://insecure.invalid",
    "https://partner.invalid",
];

// ... per node, after outlinks are chosen:
let mut external = Vec::new();
for _ in 0..spec.external_links {
    let host = EXTERNAL_HOSTS[(rng.next_u64() % EXTERNAL_HOSTS.len() as u64) as usize];
    external.push(format!("{host}/ref/{id}"));
}
// A quarter of pages mark their first outlink nofollow. Deterministic, and
// enough coverage without distorting the link graph the benchmarks measure.
let nofollow_outlinks = if rng.next_u64() % 4 == 0 && !outlinks.is_empty() {
    1
} else {
    0
};
```

Add both to the `PageNode` literal.

- [ ] **Step 4: Render them**

In `crates/pounce-bench/src/render.rs`, where outlinks are written, emit `rel="nofollow"` on the first `nofollow_outlinks` of them, and append the external anchors after:

```rust
for (i, target) in node.outlinks.iter().enumerate() {
    let rel = if i < node.nofollow_outlinks as usize {
        " rel=\"nofollow\""
    } else {
        ""
    };
    let _ = write!(
        s,
        "<a href=\"{}\"{rel}>{}</a>\n",
        graph.node(*target).path,
        graph.node(*target).title
    );
}
for url in &node.external {
    let _ = write!(s, "<a href=\"{url}\">External reference</a>\n");
}
```

- [ ] **Step 5: Run the tests and confirm they pass**

Run: `cargo test -p pounce-bench --test graph_properties --test render_output`
Expected: PASS, including the existing determinism and link-density tests.

- [ ] **Step 6: Re-take the baselines this change invalidates**

Rendered page bytes changed, so every recorded figure derived from them is stale.

```bash
cargo bench -p pounce-bench --bench parse
cargo build --release -p pounce-seo -p pounce-bench
```

Then re-run the 10k and 100k crawls exactly as `docs/benchmarks/2026-08-21-scaling-fix.md` documents, and **correct the figures in that file and in `crates/pounce-bench/README.md`.** Record the new numbers even if they are worse; a benchmark that only gets updated when it improves is an advertisement.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(bench): fixture emits external links and rel=nofollow

M2's broken-external-link and orphan-page rules need links that leave the
site, and nothing in the fixture ever left it. Hosts are .invalid so a
crawler must report them unreachable rather than reaching something real.

Rendered page bytes change, so the parse bench and crawl baselines are
re-taken here rather than discovered stale during a rule batch."
```

---

### Task 2: `PageRecord` gains `title_count` and `body_hash`

Two rules cannot fire without these. `multiple <title>` has nothing to read because the extractor discards the second title; `duplicate body` needs comparable content without retaining it.

**Files:**
- Create: `crates/pounce-parse/src/hash.rs`
- Modify: `crates/pounce-parse/src/record.rs`, `crates/pounce-parse/src/extract.rs`, `crates/pounce-parse/src/lib.rs`
- Test: `crates/pounce-parse/tests/extraction.rs`, and the golden corpus

**Interfaces:**
- Consumes: `PageRecord`, `extract(&mut PageRecord, &[u8])`
- Produces: `PageRecord { title_count: u16, body_hash: Option<u64>, .. }`, `pounce_parse::hash::fnv1a(&str) -> u64`, `Fnv1a::{new, write, finish}`

- [ ] **Step 1: Write the failing tests**

In `crates/pounce-parse/tests/extraction.rs`:

```rust
#[test]
fn a_second_title_is_counted_even_though_only_the_first_is_kept() {
    let r = parse_str("<title>First</title><title>Second</title>");
    assert_eq!(r.title.as_deref(), Some("First"));
    assert_eq!(r.title_count, 2, "the second title is a finding, not noise");
}

#[test]
fn a_single_title_counts_once_and_no_title_counts_zero() {
    assert_eq!(parse_str("<title>Only</title>").title_count, 1);
    assert_eq!(parse_str("<p>none</p>").title_count, 0);
    // An empty title is still a title that was present.
    assert_eq!(parse_str("<title></title>").title_count, 1);
}

#[test]
fn identical_body_text_hashes_identically_across_different_markup() {
    // Duplicate-content detection must survive a wrapper div or a line break,
    // or it reports every templated page as unique.
    let a = parse_str("<body><p>alpha beta gamma</p></body>");
    let b = parse_str("<body><div><span>alpha beta\n  gamma</span></div></body>");
    assert!(a.body_hash.is_some());
    assert_eq!(a.body_hash, b.body_hash);
}

#[test]
fn different_body_text_hashes_differently() {
    let a = parse_str("<body><p>alpha beta gamma</p></body>");
    let b = parse_str("<body><p>alpha beta delta</p></body>");
    assert_ne!(a.body_hash, b.body_hash);
}

#[test]
fn a_body_with_no_text_has_no_hash() {
    // None means "nothing to compare", which is different from "hash of the
    // empty string" — otherwise every empty page is a duplicate of every other.
    assert_eq!(parse_str("<body>   </body>").body_hash, None);
}

#[test]
fn script_text_does_not_reach_the_body_hash() {
    let a = parse_str("<body><p>alpha</p><script>var x = 'beta';</script></body>");
    let b = parse_str("<body><p>alpha</p></body>");
    assert_eq!(a.body_hash, b.body_hash);
}
```

In `crates/pounce-parse/src/hash.rs`, a known-answer test — the same discipline as `rng.rs`, because a persisted hash whose value drifts silently invalidates every stored `body_hash`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_published_fnv1a_vectors() {
        // From the FNV reference. If this fails, treat it as a breaking change
        // to the .pounce format, not as an expectation to update.
        assert_eq!(fnv1a(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a("a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a("foobar"), 0x8506_1526_2278_8e57);
    }

    #[test]
    fn streaming_matches_one_shot() {
        let mut h = Fnv1a::new();
        h.write("foo");
        h.write("bar");
        assert_eq!(h.finish(), fnv1a("foobar"));
    }
}
```

- [ ] **Step 2: Run and confirm red for the right reason**

Run: `cargo test -p pounce-parse`
Expected: compile error (no `title_count` field). Add the fields to `PageRecord` and a stub `hash.rs` whose functions are `todo!("T2 hash")`, plus `title_count: 0, body_hash: None` at every construction site including the test fixtures in `record_shape.rs` and `body_kinds.rs`. Re-run. Expected now: FAIL on assertions and `not yet implemented`.

- [ ] **Step 3: Implement the hash**

`crates/pounce-parse/src/hash.rs`:

```rust
//! FNV-1a, hand-rolled and pinned by known-answer tests.
//!
//! Hand-rolled for the same reason `pounce-bench` hand-rolls its PRNG: this
//! value is **persisted** in `.pounce` files, so it must produce the same
//! number in every build forever. `DefaultHasher` is SipHash with no
//! cross-release stability guarantee, which would silently make old files'
//! `body_hash` incomparable with new ones.

const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01b3;

/// Incremental FNV-1a, so text can be hashed as it streams past without ever
/// being accumulated.
#[derive(Debug, Clone)]
pub struct Fnv1a(u64);

impl Fnv1a {
    pub fn new() -> Self {
        Self(OFFSET)
    }

    pub fn write(&mut self, s: &str) {
        for byte in s.as_bytes() {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(PRIME);
        }
    }

    pub fn finish(&self) -> u64 {
        self.0
    }
}

impl Default for Fnv1a {
    fn default() -> Self {
        Self::new()
    }
}

pub fn fnv1a(s: &str) -> u64 {
    let mut h = Fnv1a::new();
    h.write(s);
    h.finish()
}
```

- [ ] **Step 4: Populate both fields in the existing pass**

In `crates/pounce-parse/src/extract.rs`, add to `State`:

```rust
    /// Every `<title>` seen, not just the one kept. A second title is a
    /// finding; discarding it silently made the rule unwritable.
    title_count: u16,
    /// Hashed as text streams past, so duplicate detection costs no memory.
    body: crate::hash::Fnv1a,
    /// Whether any non-whitespace body text was seen at all. `None` must mean
    /// "nothing to compare", not "hash of the empty string" — otherwise every
    /// blank page duplicates every other.
    body_any: bool,
```

In the `element!("title", ...)` handler, before the existing body, add:

```rust
                state.borrow_mut().title_count += 1;
```

In the `text!("body", ...)` handler, inside the existing `if s.in_code == 0` branch, after `s.words.feed(...)`, hash the same normalised text the word counter sees so markup differences do not change the hash:

```rust
                    let normalised = collapse(t.as_str());
                    if !normalised.is_empty() {
                        if s.body_any {
                            s.body.write(" ");
                        }
                        s.body.write(&normalised);
                        s.body_any = true;
                    }
```

Then in the assignment block at the end of `extract`:

```rust
    record.title_count = state.title_count;
    record.body_hash = state.body_any.then(|| state.body.finish());
```

- [ ] **Step 5: Run and confirm green, then refresh the goldens**

Run: `cargo test -p pounce-parse`
Expected: PASS.

Then `UPDATE_GOLDEN=1 cargo test -p pounce-parse --test extraction the_corpus`, and **read the diff before accepting it** — confirm every `title_count` and `body_hash` matches what the markup actually says. A golden accepted without reading asserts whatever the code happens to do.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(parse): count titles and hash body text

Two rules could not fire. multiple <title> had nothing to read because the
extractor keeps the first and discards the rest; duplicate body needed
comparable content, and retaining 500k page bodies to compare later would
undo the flat-memory property the architecture rests on.

The hash is hand-rolled FNV-1a with known-answer tests, for the same reason
pounce-bench hand-rolls its PRNG: the value is persisted in .pounce files,
so it must be identical in every build forever, and DefaultHasher offers no
cross-release guarantee.

body_hash is None rather than the empty hash when a page has no text, so a
blank page does not duplicate every other blank page."
```

---

### Task 3: `pounce-audit` crate with `Severity`, `RuleMeta`, `Issue`

**Files:**
- Create: `crates/pounce-audit/Cargo.toml`, `crates/pounce-audit/src/lib.rs`, `crates/pounce-audit/src/issue.rs`
- Test: `crates/pounce-audit/tests/issue.rs`

**Interfaces:**
- Consumes: `pounce_core::CrawlUrl`
- Produces: `Severity::{Critical, Warning, Notice}`, `Severity::as_str()`, `Severity::from_str()`, `RuleMeta { id, severity, description, remediation }`, `Issue { rule_id, severity, detail }`

`Cargo.toml`:

```toml
[package]
name = "pounce-audit"
version.workspace = true
edition.workspace = true
repository.workspace = true
publish = false
description = "Audit rule registry and issue types"

[dependencies]
pounce-core = { path = "../pounce-core" }
pounce-parse = { path = "../pounce-parse" }
pounce-store = { path = "../pounce-store" }
serde = { workspace = true }

[dev-dependencies]
serde_json.workspace = true
```

- [ ] **Step 1: Write the failing tests**

`crates/pounce-audit/tests/issue.rs`:

```rust
use pounce_audit::{Issue, RuleMeta, Severity};

#[test]
fn severity_round_trips_through_its_stored_form() {
    // Stored as text in SQL and read back by the query layer, so the two
    // directions must agree or a filter silently matches nothing.
    for s in [Severity::Critical, Severity::Warning, Severity::Notice] {
        assert_eq!(Severity::from_str(s.as_str()), Some(s));
    }
}

#[test]
fn an_unknown_severity_is_rejected_rather_than_defaulted() {
    // Defaulting would turn a corrupt or newer file into a silently
    // mis-severitied report.
    assert_eq!(Severity::from_str("catastrophic"), None);
    assert_eq!(Severity::from_str(""), None);
}

#[test]
fn severity_orders_most_urgent_first() {
    // The grid sorts by it, and "critical after notice" is a wrong report.
    assert!(Severity::Critical < Severity::Warning);
    assert!(Severity::Warning < Severity::Notice);
}

#[test]
fn there_is_no_pass_severity() {
    // Pass is a UI state for "checked, nothing found". Storing a row per rule
    // per page is 15M rows at 500k to record absence.
    assert_eq!(Severity::from_str("pass"), None);
}

#[test]
fn an_issue_carries_its_rule_and_an_optional_detail() {
    let issue = Issue {
        rule_id: "title.too-long",
        severity: Severity::Warning,
        detail: Some("84 characters".into()),
    };
    assert_eq!(issue.rule_id, "title.too-long");
    // Detail is optional: "missing title" needs no elaboration, and an empty
    // string would render as a blank cell rather than as nothing.
    let bare = Issue { detail: None, ..issue.clone() };
    assert_eq!(bare.detail, None);
}

#[test]
fn rule_metadata_carries_remediation_not_just_a_complaint() {
    let meta = RuleMeta {
        id: "title.missing",
        severity: Severity::Critical,
        description: "The page has no <title> element.",
        remediation: "Add a unique <title> of 30-60 characters.",
    };
    assert!(!meta.remediation.is_empty(), "a finding without a fix is noise");
}
```

- [ ] **Step 2: Run and confirm red**

Run: `cargo test -p pounce-audit`
Expected: the crate does not exist. Create it with `issue.rs` defining the types and every method as `todo!("T3")`. Add `"crates/*"` already covers it in the workspace. Re-run. Expected: FAIL with `not yet implemented`.

- [ ] **Step 3: Implement**

`crates/pounce-audit/src/issue.rs`:

```rust
//! What a rule declares, and what it finds.

use serde::{Deserialize, Serialize};

/// How urgent a finding is.
///
/// Ordered most-urgent-first so `sort()` puts critical at the top. There is
/// deliberately no `Pass`: the spec's fourth state is a UI rendering of
/// "checked, nothing found", and storing a row per rule per page to record
/// absence is 15M rows on a 500k crawl.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Critical,
    Warning,
    Notice,
}

impl Severity {
    /// The form stored in SQL and accepted by `--fail-on`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::Warning => "warning",
            Self::Notice => "notice",
        }
    }

    /// `None` for anything unrecognised. Never defaults: a corrupt or
    /// newer-format file must not silently become a mis-severitied report.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "critical" => Some(Self::Critical),
            "warning" => Some(Self::Warning),
            "notice" => Some(Self::Notice),
            _ => None,
        }
    }
}

/// Everything a rule declares about itself.
///
/// `id` is stable forever: it appears in `--fail-on`, in exported reports, and
/// in saved `.pounce` files. Renaming one breaks a user's CI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleMeta {
    pub id: &'static str,
    pub severity: Severity,
    /// What was found, in the user's words.
    pub description: &'static str,
    /// What to do about it. A finding without a fix is noise.
    pub remediation: &'static str,
}

/// One finding against one page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub rule_id: &'static str,
    /// Copied from the rule rather than looked up, because it is stored on the
    /// row: the grid filters millions of issues by severity, and a per-row
    /// join is exactly the query pattern M3 exists to avoid.
    pub severity: Severity,
    /// `None` when the rule id says everything. An empty string would render
    /// as a blank cell rather than as nothing.
    pub detail: Option<String>,
}
```

`crates/pounce-audit/src/lib.rs`:

```rust
//! Audit rules: what to check, and what was found.

pub mod issue;

pub use issue::{Issue, RuleMeta, Severity};
```

- [ ] **Step 4: Run and confirm green**

Run: `cargo test -p pounce-audit`
Expected: PASS, 6 tests.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(audit): the vocabulary a rule speaks

Severity is ordered most-urgent-first so a sort puts critical at the top,
and from_str returns None rather than defaulting, because defaulting turns
a corrupt or newer file into a silently mis-severitied report.

There is no Pass variant. The spec's fourth state is a UI rendering of
'checked, nothing found', and storing a row per rule per page to record
absence is 15M rows on a 500k crawl.

Issue carries severity by value rather than by reference to its rule: the
grid filters millions of rows by it, and a per-row join is the query
pattern M3 exists to avoid."
```

---

### Task 4: `PageRule` trait and the registry

**Files:**
- Create: `crates/pounce-audit/src/rule.rs`, `crates/pounce-audit/src/registry.rs`
- Modify: `crates/pounce-audit/src/lib.rs`
- Test: `crates/pounce-audit/tests/registry.rs`

**Interfaces:**
- Consumes: `RuleMeta`, `Issue`, `pounce_parse::PageRecord`
- Produces: `trait PageRule { fn meta(&self) -> RuleMeta; fn check(&self, page: &PageRecord, out: &mut Vec<Issue>); }`, `Registry::{new, register_page, page_rules, len, run_page}`, `RegistryError::{DuplicateId, MalformedId, CapExceeded}`

- [ ] **Step 1: Write the failing tests**

`crates/pounce-audit/tests/registry.rs`:

```rust
use pounce_audit::{Issue, PageRule, Registry, RegistryError, RuleMeta, Severity};
use pounce_core::CrawlUrl;
use pounce_parse::{BodyKind, MetaRobots, PageRecord};

fn blank(url: &str) -> PageRecord {
    PageRecord {
        url: CrawlUrl::parse(url).unwrap(),
        status: 200,
        depth: 0,
        size: 0,
        truncated: false,
        content_type: Some("text/html".into()),
        charset: Some("utf-8".into()),
        kind: BodyKind::Html,
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

struct MissingTitle;
impl PageRule for MissingTitle {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "title.missing",
            severity: Severity::Critical,
            description: "The page has no <title> element.",
            remediation: "Add a unique <title> of 30-60 characters.",
        }
    }
    fn check(&self, page: &PageRecord, out: &mut Vec<Issue>) {
        if page.title.is_none() {
            out.push(Issue {
                rule_id: self.meta().id,
                severity: self.meta().severity,
                detail: None,
            });
        }
    }
}

#[test]
fn a_registered_rule_runs_against_a_page() {
    let mut reg = Registry::new();
    reg.register_page(Box::new(MissingTitle)).unwrap();
    let mut record = blank("https://example.com/a");
    record.title = None;

    let issues = reg.run_page(&record);
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].rule_id, "title.missing");
}

#[test]
fn a_rule_that_does_not_fire_produces_nothing() {
    let mut reg = Registry::new();
    reg.register_page(Box::new(MissingTitle)).unwrap();
    let mut record = blank("https://example.com/a");
    record.title = Some("A title".into());
    assert!(reg.run_page(&record).is_empty());
}

#[test]
fn duplicate_ids_are_rejected() {
    // Two rules sharing an id makes per-rule counts wrong and --fail-on
    // ambiguous, and it is trivially easy to do by copy-paste.
    let mut reg = Registry::new();
    reg.register_page(Box::new(MissingTitle)).unwrap();
    assert!(matches!(
        reg.register_page(Box::new(MissingTitle)),
        Err(RegistryError::DuplicateId("title.missing"))
    ));
}

#[test]
fn ids_must_follow_the_stable_naming_shape() {
    struct Bad;
    impl PageRule for Bad {
        fn meta(&self) -> RuleMeta {
            RuleMeta {
                id: "Title Missing",
                severity: Severity::Notice,
                description: "x",
                remediation: "y",
            }
        }
        fn check(&self, _p: &PageRecord, _o: &mut Vec<Issue>) {}
    }
    let mut reg = Registry::new();
    assert!(matches!(
        reg.register_page(Box::new(Bad)),
        Err(RegistryError::MalformedId("Title Missing"))
    ));
}

#[test]
fn the_thirty_rule_cap_is_enforced() {
    // A hard invariant: racing a competitor's feature list is the identified
    // primary failure mode, and a cap that is only documented is not a cap.
    let mut reg = Registry::new();
    for i in 0..30 {
        let id: &'static str = Box::leak(format!("test.rule-{i}").into_boxed_str());
        reg.register_page(Box::new(Generated(id))).unwrap();
    }
    assert_eq!(reg.len(), 30);
    let extra: &'static str = Box::leak("test.rule-30".to_string().into_boxed_str());
    assert!(matches!(
        reg.register_page(Box::new(Generated(extra))),
        Err(RegistryError::CapExceeded)
    ));
}

struct Generated(&'static str);
impl PageRule for Generated {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: self.0,
            severity: Severity::Notice,
            description: "generated",
            remediation: "generated",
        }
    }
    fn check(&self, _p: &PageRecord, _o: &mut Vec<Issue>) {}
}
```

The literal is repeated here rather than shared with `pounce-parse`'s tests: a helper crate existing solely to hold one struct literal is not worth the dependency edge.

- [ ] **Step 2: Run and confirm red**

Run: `cargo test -p pounce-audit --test registry`
Expected: compile error. Create `rule.rs` and `registry.rs` with the types and `todo!("T4")` bodies, re-run, and expect FAIL with `not yet implemented`.

- [ ] **Step 3: Implement**

`crates/pounce-audit/src/rule.rs`:

```rust
//! The two shapes a rule can take.

use crate::issue::{Issue, RuleMeta};
use pounce_parse::PageRecord;

/// A check that needs nothing but the page in front of it.
///
/// Deliberately handed a `&PageRecord` and nothing else. It **cannot** issue a
/// query, which is what keeps Gate M2's 10% wall-time budget enforceable by
/// the compiler rather than by convention — and at rule 23 of 30, a convention
/// would have lost.
pub trait PageRule: Send + Sync {
    fn meta(&self) -> RuleMeta;
    /// Push one `Issue` per finding. Runs on the crawl's hot path: no I/O, and
    /// no allocation beyond the issues themselves.
    fn check(&self, page: &PageRecord, out: &mut Vec<Issue>);
}
```

`crates/pounce-audit/src/registry.rs`:

```rust
//! Every rule, and the invariants that are about the set rather than any one.

use crate::issue::Issue;
use crate::rule::PageRule;
use pounce_parse::PageRecord;
use std::collections::HashSet;

/// v0.1's hard cap. Feature parity is explicitly not the goal; racing a list we
/// are behind on is the identified primary failure mode.
pub const MAX_RULES: usize = 30;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RegistryError {
    #[error("rule id `{0}` is already registered")]
    DuplicateId(&'static str),
    #[error("rule id `{0}` must look like `batch.rule-name`")]
    MalformedId(&'static str),
    #[error("v0.1 caps at {MAX_RULES} rules")]
    CapExceeded,
}

#[derive(Default)]
pub struct Registry {
    page_rules: Vec<Box<dyn PageRule>>,
    ids: HashSet<&'static str>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_page(&mut self, rule: Box<dyn PageRule>) -> Result<(), RegistryError> {
        let id = rule.meta().id;
        self.claim(id)?;
        self.page_rules.push(rule);
        Ok(())
    }

    fn claim(&mut self, id: &'static str) -> Result<(), RegistryError> {
        if !valid_id(id) {
            return Err(RegistryError::MalformedId(id));
        }
        if self.ids.contains(id) {
            return Err(RegistryError::DuplicateId(id));
        }
        if self.len() >= MAX_RULES {
            return Err(RegistryError::CapExceeded);
        }
        self.ids.insert(id);
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.page_rules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn page_rules(&self) -> &[Box<dyn PageRule>] {
        &self.page_rules
    }

    /// Runs every page rule against one record.
    pub fn run_page(&self, page: &PageRecord) -> Vec<Issue> {
        let mut out = Vec::new();
        for rule in &self.page_rules {
            rule.check(page, &mut out);
        }
        out
    }
}

/// `batch.rule-name`: lowercase, one dot, hyphens allowed after it.
///
/// Enforced because ids are permanent — they appear in `--fail-on`, in
/// exported reports, and in saved files — so a typo caught at registration is
/// far cheaper than one caught by a user's CI.
fn valid_id(id: &str) -> bool {
    let Some((batch, name)) = id.split_once('.') else {
        return false;
    };
    !batch.is_empty()
        && !name.is_empty()
        && batch.chars().all(|c| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}
```

Add `thiserror.workspace = true` to `crates/pounce-audit/Cargo.toml`, and re-export from `lib.rs`:

```rust
pub mod registry;
pub mod rule;

pub use registry::{MAX_RULES, Registry, RegistryError};
pub use rule::PageRule;
```

- [ ] **Step 4: Run and confirm green**

Run: `cargo test -p pounce-audit`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(audit): PageRule and the registry that owns the set invariants

PageRule is handed a &PageRecord and nothing else, so it cannot issue a
query. That is the point: Gate M2 allows rules 10% of crawl wall time,
which after the scaling fix is ~14 seconds at 500k, and a trait that could
reach the database would make that budget a convention rather than a
compiler-checked fact.

The registry owns what is true of the set rather than any member: ids are
unique, ids match batch.rule-name, and the total is capped at 30. Ids are
permanent — they appear in --fail-on, exports and saved files — so a typo
caught at registration is far cheaper than one caught by a user's CI, and a
cap that is only documented is not a cap."
```

---

### Task 5: `SiteRule` trait, held by the same registry

**Files:**
- Modify: `crates/pounce-audit/src/rule.rs`, `crates/pounce-audit/src/registry.rs`, `crates/pounce-audit/src/lib.rs`
- Test: `crates/pounce-audit/tests/registry.rs`

**Interfaces:**
- Consumes: `Registry`, `RuleMeta`, `Issue`, `pounce_store::{Store, StoreError}`
- Produces: `trait SiteRule { fn meta(&self) -> RuleMeta; fn check(&self, store: &Store) -> Result<Vec<(String, Issue)>, StoreError>; }`, `Registry::{register_site, run_site}`

`SiteRule::check` returns `(url, Issue)` pairs because a site rule discovers *which* pages are affected, unlike a page rule which is already looking at one.

- [ ] **Step 1: Write the failing tests**

Append to `crates/pounce-audit/tests/registry.rs`:

```rust
use pounce_audit::SiteRule;
use pounce_store::Store;

struct DuplicateTitle;
impl SiteRule for DuplicateTitle {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "title.duplicate",
            severity: Severity::Warning,
            description: "More than one page shares this <title>.",
            remediation: "Give each page a title describing only that page.",
        }
    }
    fn check(&self, store: &Store) -> Result<Vec<(String, Issue)>, pounce_store::StoreError> {
        let mut stmt = store.conn().prepare(
            "SELECT url, title FROM pages WHERE title IS NOT NULL AND title IN \
             (SELECT title FROM pages WHERE title IS NOT NULL \
              GROUP BY title HAVING count(*) > 1)",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (url, title) = row?;
            out.push((
                url,
                Issue {
                    rule_id: self.meta().id,
                    severity: self.meta().severity,
                    detail: Some(title),
                },
            ));
        }
        Ok(out)
    }
}

#[test]
fn a_site_rule_sees_across_pages() {
    let mut store = Store::in_memory().unwrap();
    seed_titles(&mut store, &[("https://e.com/a", "Shared"), ("https://e.com/b", "Shared"), ("https://e.com/c", "Unique")]);

    let mut reg = Registry::new();
    reg.register_site(Box::new(DuplicateTitle)).unwrap();

    let issues = reg.run_site(&store).unwrap();
    assert_eq!(issues.len(), 2, "both sharers are findings, not just one");
    assert!(issues.iter().all(|(_, i)| i.rule_id == "title.duplicate"));
    assert!(!issues.iter().any(|(url, _)| url.ends_with("/c")));
}

#[test]
fn page_and_site_rules_share_one_id_space_and_one_count() {
    // "30 rules" has to be one number, or the cap is meaningless.
    let mut reg = Registry::new();
    reg.register_page(Box::new(MissingTitle)).unwrap();
    reg.register_site(Box::new(DuplicateTitle)).unwrap();
    assert_eq!(reg.len(), 2);

    struct Clashing;
    impl SiteRule for Clashing {
        fn meta(&self) -> RuleMeta { MissingTitle.meta() }
        fn check(&self, _s: &Store) -> Result<Vec<(String, Issue)>, pounce_store::StoreError> {
            Ok(vec![])
        }
    }
    assert!(matches!(
        reg.register_site(Box::new(Clashing)),
        Err(RegistryError::DuplicateId("title.missing"))
    ));
}
```

Add the seed helper in the same file:

```rust
fn seed_titles(store: &mut Store, rows: &[(&str, &str)]) {
    use pounce_store::Writer;
    let mut writer = Writer::with_batch_size(store, rows.len().max(1));
    for (url, title) in rows {
        let mut record = blank(url);
        record.title = Some((*title).to_string());
        writer.push(&record).unwrap();
    }
    writer.flush().unwrap();
}
```

- [ ] **Step 2: Run and confirm red**

Run: `cargo test -p pounce-audit --test registry`
Expected: compile error (no `SiteRule`). Add the trait and `todo!("T5")` registry methods, re-run, expect FAIL with `not yet implemented`.

- [ ] **Step 3: Implement**

Append to `crates/pounce-audit/src/rule.rs`:

```rust
use pounce_store::{Store, StoreError};

/// A check that needs the whole crawl.
///
/// Runs once, after the last page lands, against indexed columns. That is not
/// the "post-pass" T2.2 forbids: the prohibition is on a second pass over page
/// *bodies*, and these are `GROUP BY`/join queries that touch none and
/// allocate nothing proportional to crawl size.
///
/// Returns `(url, Issue)` because a site rule discovers *which* pages are
/// affected, where a page rule is already looking at one.
pub trait SiteRule: Send + Sync {
    fn meta(&self) -> RuleMeta;
    fn check(&self, store: &Store) -> Result<Vec<(String, Issue)>, StoreError>;
}
```

In `registry.rs`, add the field, registration, and runner:

```rust
    site_rules: Vec<Box<dyn SiteRule>>,
```

```rust
    pub fn register_site(&mut self, rule: Box<dyn SiteRule>) -> Result<(), RegistryError> {
        let id = rule.meta().id;
        self.claim(id)?;
        self.site_rules.push(rule);
        Ok(())
    }

    pub fn site_rules(&self) -> &[Box<dyn SiteRule>] {
        &self.site_rules
    }

    /// Runs every site rule against the finished crawl.
    pub fn run_site(&self, store: &Store) -> Result<Vec<(String, Issue)>, StoreError> {
        let mut out = Vec::new();
        for rule in &self.site_rules {
            out.extend(rule.check(store)?);
        }
        Ok(out)
    }
```

and change `len` so the cap spans both:

```rust
    pub fn len(&self) -> usize {
        self.page_rules.len() + self.site_rules.len()
    }
```

- [ ] **Step 4: Run and confirm green**

Run: `cargo test -p pounce-audit`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(audit): SiteRule, sharing the registry's id space and cap

Thirteen of the thirty rules need data from pages that may not be crawled
yet, so a trait shaped around one PageRecord would leave them homeless.
SiteRule runs once against the finished database.

That is not the post-pass T2.2 forbids: the prohibition is on a second pass
over page bodies, and these are GROUP BY and join queries over indexed
columns that touch no bodies and allocate nothing proportional to crawl
size.

One registry holds both so 'thirty rules' stays one number and one id
space; a page rule and a site rule sharing an id is rejected the same way
two page rules are."
```

---

### Task 6: `issues` table and writer support

**Files:**
- Create: `crates/pounce-store/src/migrations/008_issues.sql`
- Modify: `crates/pounce-store/src/schema.rs`, `crates/pounce-store/src/writer.rs`
- Test: `crates/pounce-store/tests/writer.rs`, `crates/pounce-store/tests/schema.rs`

**Interfaces:**
- Consumes: `Writer`, `Store::build_query_indices`
- Produces: `Writer::push(&PageRecord) -> Result<i64, StoreError>` (**changed** from `Result<()>`), `Writer::issues(page_id: i64, issues: &[(&'static str, &'static str, Option<&str>)]) -> Result<(), StoreError>`

`issues()` takes `(rule_id, severity_str, detail)` tuples rather than `pounce_audit::Issue`, so `pounce-store` does not depend on `pounce-audit` — the dependency runs the other way, and reversing it would make the two mutually dependent.

- [ ] **Step 1: Write the failing tests**

Append to `crates/pounce-store/tests/writer.rs`:

```rust
#[test]
fn issues_commit_in_the_same_transaction_as_their_page() {
    // There must be no state where a page exists with half its findings.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c.pounce");
    {
        let mut store = Store::open(&path).unwrap();
        let mut writer = Writer::with_batch_size(&mut store, 10);
        let id = writer.push(&record("https://example.com/a")).unwrap();
        writer
            .issues(id, &[("title.missing", "critical", None)])
            .unwrap();
        // Dropped without flush: the shape of a crawl that was killed.
    }
    let reopened = Store::open(&path).unwrap();
    let pages: i64 = reopened
        .conn()
        .query_row("SELECT count(*) FROM pages", [], |r| r.get(0))
        .unwrap();
    let issues: i64 = reopened
        .conn()
        .query_row("SELECT count(*) FROM issues", [], |r| r.get(0))
        .unwrap();
    assert_eq!((pages, issues), (0, 0), "page and issues roll back together");
}

#[test]
fn an_issue_stores_its_rule_severity_and_detail() {
    let mut store = Store::in_memory().unwrap();
    {
        let mut writer = Writer::with_batch_size(&mut store, 4);
        let id = writer.push(&record("https://example.com/a")).unwrap();
        writer
            .issues(
                id,
                &[
                    ("title.too-long", "warning", Some("84 characters")),
                    ("image.missing-alt", "warning", None),
                ],
            )
            .unwrap();
        writer.flush().unwrap();
    }
    let rows: Vec<(String, String, Option<String>)> = store
        .conn()
        .prepare("SELECT rule_id, severity, detail FROM issues ORDER BY rule_id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(rows[0], ("image.missing-alt".into(), "warning".into(), None));
    assert_eq!(
        rows[1],
        ("title.too-long".into(), "warning".into(), Some("84 characters".into()))
    );
}

#[test]
fn push_returns_the_same_id_when_a_url_is_re_fetched() {
    // Resume re-fetches. If the id changed, the issues written against the old
    // one would be orphaned by the ON DELETE CASCADE.
    let mut store = Store::in_memory().unwrap();
    let mut writer = Writer::with_batch_size(&mut store, 4);
    let first = writer.push(&record("https://example.com/a")).unwrap();
    let second = writer.push(&record("https://example.com/a")).unwrap();
    assert_eq!(first, second);
    writer.flush().unwrap();
}
```

Append to `crates/pounce-store/tests/schema.rs`:

```rust
#[test]
fn issue_indices_are_deferred_like_every_other_query_index() {
    // An index nothing reads during a crawl is not maintained during one.
    let store = Store::in_memory().unwrap();
    let count = |s: &Store, name: &str| -> i64 {
        scalar(
            s.conn(),
            &format!("SELECT count(*) FROM sqlite_master WHERE type='index' AND name='{name}'"),
        )
    };
    for name in ["issues_page", "issues_rule", "issues_severity"] {
        assert_eq!(count(&store, name), 0, "{name} must not exist during a crawl");
    }
    store.build_query_indices().unwrap();
    for name in ["issues_page", "issues_rule", "issues_severity"] {
        assert_eq!(count(&store, name), 1, "{name} must exist after the crawl");
    }
}
```

- [ ] **Step 2: Run and confirm red**

Run: `cargo test -p pounce-store`
Expected: FAIL — no `issues` table, and `push` returns `()`.

- [ ] **Step 3: Add the migration**

`crates/pounce-store/src/migrations/008_issues.sql`:

```sql
-- One row per finding.
--
-- `severity` is denormalised rather than joined from rule metadata: the grid
-- filters millions of issues by it, and a per-row join is exactly the query
-- pattern M3 exists to avoid. It also makes a `.pounce` file self-contained —
-- it records what was found at crawl time, and re-grading a rule later does
-- not silently rewrite history.
--
-- No index here. `issues_page`, `issues_rule` and `issues_severity` are built
-- by `Store::build_query_indices` after the crawl, following the rule the 500k
-- measurement taught: an index nothing reads during a crawl is not maintained
-- during one.
CREATE TABLE issues (
    id       INTEGER PRIMARY KEY,
    page_id  INTEGER NOT NULL REFERENCES pages (id) ON DELETE CASCADE,
    rule_id  TEXT    NOT NULL,
    severity TEXT    NOT NULL,
    detail   TEXT
) STRICT;
```

Register it in `schema.rs`'s `MIGRATIONS`, and extend `build_query_indices`:

```rust
        self.conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS links_target ON links (target_url);
             CREATE INDEX IF NOT EXISTS issues_page ON issues (page_id);
             CREATE INDEX IF NOT EXISTS issues_rule ON issues (rule_id);
             CREATE INDEX IF NOT EXISTS issues_severity ON issues (severity)",
        )?;
```

- [ ] **Step 4: Make `push` return the id and add `issues`**

In `writer.rs`, change the upsert to return the row id and the signature to `Result<i64, StoreError>`. The upsert already preserves the id on conflict, so `RETURNING id` gives the same value on a re-fetch:

```rust
    pub fn push(&mut self, record: &PageRecord) -> Result<i64, StoreError> {
        // ... unchanged body up to the statement ...
        let id: i64 = stmt.query_row(params![/* unchanged */], |r| r.get(0))?;
        drop(stmt);

        self.in_batch += 1;
        if self.in_batch >= self.batch_size {
            self.flush()?;
        }
        Ok(id)
    }
```

with `RETURNING id` appended to `insert_sql()`'s generated statement. Then:

```rust
    /// Appends findings for a page inside the open batch.
    ///
    /// Takes plain tuples rather than `pounce_audit::Issue` so that
    /// `pounce-store` does not depend on `pounce-audit`; the dependency runs
    /// the other way, and reversing it would make the two mutually dependent.
    pub fn issues(
        &mut self,
        page_id: i64,
        issues: &[(&'static str, &'static str, Option<&str>)],
    ) -> Result<(), StoreError> {
        if issues.is_empty() {
            return Ok(());
        }
        self.begin()?;
        let mut stmt = self.store.conn().prepare_cached(
            "INSERT INTO issues (page_id, rule_id, severity, detail) VALUES (?1, ?2, ?3, ?4)",
        )?;
        for (rule_id, severity, detail) in issues {
            stmt.execute(params![page_id, rule_id, severity, detail])?;
        }
        Ok(())
    }
```

Update the two existing `writer.push(...)` call sites in `crates/pounce-cli/src/lib.rs` and any test that binds its result.

- [ ] **Step 5: Run and confirm green**

Run: `cargo test --workspace --lib --bins --tests -- --test-threads=1`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(store): store issues alongside the pages that have them

Issues commit in the same transaction as their page, so there is no state
where a page exists with half its findings. push therefore returns the page
id — and because the upsert preserves the row on conflict, a resumed crawl
re-fetching a URL gets the same id back rather than orphaning the issues
written against the old one.

severity is denormalised onto the row. The grid filters millions of issues
by it, and a per-row join to rule metadata is the query pattern M3 exists
to avoid; it also keeps a .pounce file self-contained, recording what was
found at crawl time rather than what the current build would call it.

Its indices are built by build_query_indices, not the migration, following
what the 500k measurement taught: an index nothing reads during a crawl is
not maintained during one.

issues() takes plain tuples rather than pounce_audit::Issue so the store
does not depend on the audit crate; that dependency runs the other way."
```

---

### Task 7: Run the rules during and after a crawl

**Files:**
- Modify: `crates/pounce-cli/Cargo.toml`, `crates/pounce-cli/src/lib.rs`
- Test: `crates/pounce-cli/src/lib.rs` (the existing end-to-end fixture test)

**Interfaces:**
- Consumes: `Registry::{run_page, run_site}`, `Writer::{push, issues}`, `Store::build_query_indices`
- Produces: `crawl(seed, output, registry)` — the existing `crawl` gains a `&Registry` parameter

- [ ] **Step 1: Write the failing test**

In the test module of `crates/pounce-cli/src/lib.rs`, extend the existing fixture crawl test:

```rust
#[tokio::test]
async fn a_crawl_records_the_issues_its_rules_find() {
    let (base_url, _paths, server) = spawn_fixture().await;
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("audited.pounce");
    let seed = CrawlUrl::parse(&base_url).unwrap();

    let mut registry = Registry::new();
    registry.register_page(Box::new(NoindexRule)).unwrap();

    crawl(seed, &output, &registry).await.unwrap();

    let store = Store::open(&output).unwrap();
    let found: i64 = store
        .conn()
        .query_row(
            "SELECT count(*) FROM issues WHERE rule_id = 'indexability.noindex'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let noindex_pages: i64 = store
        .conn()
        .query_row("SELECT count(*) FROM pages WHERE noindex = 1", [], |r| r.get(0))
        .unwrap();
    assert!(noindex_pages > 0, "the fixture seeds noindex pages");
    assert_eq!(found, noindex_pages, "one issue per noindex page, no more");

    server.abort();
}

#[tokio::test]
async fn an_empty_registry_records_no_issues_and_still_crawls() {
    let (base_url, _paths, server) = spawn_fixture().await;
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("plain.pounce");
    let seed = CrawlUrl::parse(&base_url).unwrap();

    let summary = crawl(seed, &output, &Registry::new()).await.unwrap();
    assert!(summary.pages > 0);

    let store = Store::open(&output).unwrap();
    let issues: i64 = store
        .conn()
        .query_row("SELECT count(*) FROM issues", [], |r| r.get(0))
        .unwrap();
    assert_eq!(issues, 0);

    server.abort();
}

struct NoindexRule;
impl PageRule for NoindexRule {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: "indexability.noindex",
            severity: Severity::Critical,
            description: "The page asks search engines not to index it.",
            remediation: "Remove the noindex directive if the page should rank.",
        }
    }
    fn check(&self, page: &PageRecord, out: &mut Vec<Issue>) {
        if page.meta_robots.noindex {
            out.push(Issue {
                rule_id: self.meta().id,
                severity: self.meta().severity,
                detail: None,
            });
        }
    }
}
```

- [ ] **Step 2: Run and confirm red**

Run: `cargo test -p pounce-seo`
Expected: compile error (`crawl` takes two arguments). Add the `registry: &Registry` parameter and have the writer stage ignore it, re-run, and expect FAIL with `assertion failed: found == noindex_pages` (0 vs a positive number).

- [ ] **Step 3: Wire page rules into the writer stage**

Add `pounce-audit = { path = "../pounce-audit" }` to `crates/pounce-cli/Cargo.toml`. In the writer closure of `crawl`, replace the `writer.push(&record)?;` line:

```rust
                        let issues = registry.run_page(&record);
                        let page_id = writer.push(&record)?;
                        if !issues.is_empty() {
                            let rows: Vec<(&'static str, &'static str, Option<&str>)> = issues
                                .iter()
                                .map(|i| {
                                    (i.rule_id, i.severity.as_str(), i.detail.as_deref())
                                })
                                .collect();
                            writer.issues(page_id, &rows)?;
                        }
```

- [ ] **Step 4: Run site rules after the crawl, before the indices**

Replace the tail of `crawl`:

```rust
    writer.flush()?;
    // The writer holds `&mut store` for its whole lifetime, so it has to go
    // before a site rule can read the database.
    drop(writer);

    // Site rules need the crawl-time indices and produce rows that should be
    // indexed with everything else, so they run between the two.
    let site_issues = registry.run_site(&store)?;
    if !site_issues.is_empty() {
        let mut writer = Writer::new(&mut store);
        for (url, issue) in &site_issues {
            // The page is already stored, so this resolves rather than inserts.
            if let Some(page_id) = writer.page_id(url)? {
                writer.issues(
                    page_id,
                    &[(issue.rule_id, issue.severity.as_str(), issue.detail.as_deref())],
                )?;
            }
        }
        writer.flush()?;
    }

    store.build_query_indices()?;

    Ok(CrawlSummary { pages, failures })
```

This needs one more store method — add it in `writer.rs` alongside `issues`:

```rust
    /// The stored id for a URL, or `None` if it was never crawled.
    ///
    /// Only used by site rules, which run once at the end over a bounded
    /// result set — never on the per-page hot path.
    pub fn page_id(&mut self, url: &str) -> Result<Option<i64>, StoreError> {
        self.begin()?;
        let mut stmt = self
            .store
            .conn()
            .prepare_cached("SELECT id FROM pages WHERE url = ?1")?;
        match stmt.query_row(params![url], |r| r.get(0)) {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
```

- [ ] **Step 5: Run and confirm green**

Run: `cargo test --workspace --lib --bins --tests -- --test-threads=1`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(cli): run audit rules during and after the crawl

Page rules run in the writer stage, where the record exists and its issues
can go into the same transaction as the page. Site rules run after the
crawl loop but before build_query_indices, so they can use the crawl-time
indices and their own output gets indexed with everything else.

crawl takes a &Registry rather than building one, so a caller can run with
no rules at all — which is what makes the 10% budget measurable as a
difference rather than an assertion."
```

---

### Task 8: Measure the 10% budget

Gate M2 says rule execution adds under 10% to crawl wall time. After the scaling fix that is **~14 seconds at 500k**, and a gate with a number needs a measurement rather than an assurance.

**Files:**
- Create: `crates/pounce-bench/benches/audit.rs`
- Modify: `crates/pounce-bench/Cargo.toml`
- Create: `docs/benchmarks/YYYY-MM-DD-audit-rule-overhead.md`

**Interfaces:**
- Consumes: `Registry`, `PageRule`, `PageRecord`, `parse_body`

- [ ] **Step 1: Write the bench**

`crates/pounce-bench/benches/audit.rs` — measures the rule pass alone, over records built before the timed section, comparing an empty registry against a full one:

```rust
//! What audit rules cost on the crawl's hot path.
//!
//! Gate M2 allows rule execution 10% of crawl wall time. Records are built and
//! parsed before the timed section, so this measures the rule pass and nothing
//! else. An empty registry is the control: the difference between the two bars
//! is the entire budget question.

use criterion::{Criterion, criterion_group, criterion_main};
use pounce_audit::{Issue, PageRule, Registry, RuleMeta, Severity};
use pounce_parse::PageRecord;
use std::hint::black_box;

struct Cheap(&'static str);
impl PageRule for Cheap {
    fn meta(&self) -> RuleMeta {
        RuleMeta {
            id: self.0,
            severity: Severity::Warning,
            description: "bench",
            remediation: "bench",
        }
    }
    fn check(&self, page: &PageRecord, out: &mut Vec<Issue>) {
        if page.title.as_deref().is_none_or(|t| t.len() > 60) {
            out.push(Issue {
                rule_id: self.0,
                severity: Severity::Warning,
                detail: None,
            });
        }
    }
}

fn bench_rules(c: &mut Criterion) {
    let records = super_records(5_000);

    let empty = Registry::new();
    let mut full = Registry::new();
    for i in 0..30 {
        let id: &'static str = Box::leak(format!("bench.rule-{i}").into_boxed_str());
        full.register_page(Box::new(Cheap(id))).unwrap();
    }

    let mut group = c.benchmark_group("audit");
    group.throughput(criterion::Throughput::Elements(records.len() as u64));
    group.bench_function("no_rules", |b| {
        b.iter(|| {
            for r in &records {
                black_box(empty.run_page(black_box(r)));
            }
        })
    });
    group.bench_function("thirty_rules", |b| {
        b.iter(|| {
            for r in &records {
                black_box(full.run_page(black_box(r)));
            }
        })
    });
    group.finish();
}
```

`super_records` is defined in the same file. It is a near-copy of the helper in `benches/store.rs` rather than a shared one, because the two benches will diverge as the real rules land:

```rust
use pounce_bench::graph::{GraphSpec, SiteGraph};
use pounce_bench::render::render_page;
use pounce_core::CrawlUrl;
use pounce_parse::{BodyKind, MetaRobots, parse_body};

const BASE: &str = "http://localhost:8080";

/// Fully-parsed records, built once outside the timed loop.
fn super_records(pages: u32) -> Vec<PageRecord> {
    let graph = SiteGraph::generate(&GraphSpec {
        seed: 42,
        page_count: pages,
        ..GraphSpec::default()
    });
    (0..pages)
        .map(|id| {
            let html = render_page(&graph, id, BASE);
            let url =
                CrawlUrl::parse(&format!("{BASE}{}", graph.nodes[id as usize].path)).unwrap();
            let mut record = PageRecord {
                url,
                status: 200,
                depth: 2,
                size: html.len(),
                truncated: false,
                content_type: Some("text/html".into()),
                charset: Some("utf-8".into()),
                kind: BodyKind::Html,
                content_type_mismatch: false,
                elapsed_ms: 7,
                time_to_headers_ms: 3,
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
            };
            parse_body(&mut record, html.as_bytes()).unwrap();
            record
        })
        .collect()
}
```

Register the bench in `Cargo.toml`:

```toml
[[bench]]
name = "audit"
harness = false
```

and add `pounce-audit = { path = "../pounce-audit" }` to `[dev-dependencies]`.

- [ ] **Step 2: Smoke it**

Run: `cargo bench -p pounce-bench --bench audit -- --test`
Expected: `Success` for both cases. This is also what CI runs, so it must pass before commit.

- [ ] **Step 3: Take the measurement**

Run: `cargo bench -p pounce-bench --bench audit`

Then compute the answer the gate actually asks: **rule time as a percentage of the 500k crawl's 142.6 s.** Thirty rules over 500k pages is 15M evaluations; scale the per-5,000 figure accordingly.

- [ ] **Step 4: Record it**

Write `docs/benchmarks/YYYY-MM-DD-audit-rule-overhead.md` with the host, the command, both figures, the derived percentage, and — required — what the number does **not** establish: that these are trivial field checks rather than the real 30 rules, that site rules are not included, and that the real answer only arrives when the batches land. Update Gate M2 in `PLAN.md` with the measured figure.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "bench(audit): measure what rules cost on the hot path

Gate M2 allows rule execution 10% of crawl wall time, which after the
scaling fix is ~14 seconds at 500k. A gate with a number needs a
measurement, and an empty registry is the control that makes the difference
between the two bars the entire budget question.

Records are parsed before the timed section so this measures the rule pass
and nothing else. The writeup states what it does not establish: these are
trivial field checks, not the real thirty, and site rules are excluded."
```

---

## Not in this plan

Three follow-on plans, each producing working software on its own:

1. **Image HEAD fetching and the `resources` table** — a crawl capability with its own concurrency budget and skip flag, needed by exactly two rules. Must show page-crawl throughput does not regress.
2. **The six rule batches** — one plan per batch of five, each with its triggering and non-triggering fixture. They are independently reviewable and independently shippable.
3. **Gate M2 verification** — 30 rules, 60 fixtures, a hand-verified issue count over the full fixture crawl, and the real overhead figure.
