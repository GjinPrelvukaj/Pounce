# Audit rule engine — design (M2)

**Date:** 2026-08-21
**Status:** approved design, not yet implemented
**Covers:** T2.1 (`Rule` trait + registry), T2.2 (incremental execution), T2.3
(issue storage), and the shape every rule in T2.4–T2.9 must fit.

---

## 1. The problem the design exists to solve

The 30 rules in `PLAN.md` are not one kind of check. Classifying each by the
data it needs:

| Batch | Per-page | Needs other pages |
|---|---|---|
| Response | 4xx, 5xx, chains >2 hops, loops, mixed-content | — |
| Titles | missing, too long, too short, multiple `<title>` | duplicate |
| Descriptions | missing, too long, too short, truncated entity | duplicate |
| Headings & content | missing H1, multiple H1, empty H1, thin content | duplicate body |
| Indexability | `noindex`, self-referencing mismatch | canonical→non-200, canonical chain, blocked-but-linked |
| Media & links | missing alt | broken image, oversized image, broken internal link, orphan page |

**17 per-page, 13 cross-page.** A trait shaped around "one `PageRecord` in,
issues out" leaves 13 rules with nowhere to live.

Two constraints bound the answer:

- **Gate M2: rule execution adds under 10% to crawl wall time.** After the
  scaling fix that is **~14 seconds** at 500k, for roughly 15M rule
  evaluations. It rules out per-page SQL.
- **Storage stays disk-backed and memory flat.** Holding 500k page bodies to
  compare later would undo the property the architecture is built on.

## 2. Decision: two traits, one registry

```rust
/// Everything a rule declares about itself. Stable across releases: `id`
/// appears in `--fail-on`, exported reports, and saved `.pounce` files.
pub struct RuleMeta {
    pub id: &'static str,          // "title.missing", "response.5xx"
    pub severity: Severity,        // Critical | Warning | Notice
    pub description: &'static str, // what was found
    pub remediation: &'static str, // what to do about it
}

/// Runs on the crawl's hot path, once per page, with no access to anything
/// but the record in front of it.
pub trait PageRule: Send + Sync {
    fn meta(&self) -> RuleMeta;
    fn check(&self, page: &PageRecord, out: &mut Vec<Issue>);
}

/// Runs once, after the crawl, against the finished database.
pub trait SiteRule: Send + Sync {
    fn meta(&self) -> RuleMeta;
    fn check(&self, store: &Store) -> Result<Vec<Issue>, StoreError>;
}
```

**Why two traits rather than one with an optional `finalize`: the split puts
the performance budget in the type system.** A `PageRule` is handed a
`&PageRecord` and nothing else, so it *cannot* issue a query. The 10% gate
cannot be blown by a rule that quietly does per-page SQL — and at rule 23 of
30, a convention would have lost. The two also test differently: a `PageRule`
tests against a `PageRecord` literal with no database at all, a `SiteRule`
needs a seeded one. Forcing both into one shape makes both harnesses worse.

**One registry holds both.** It owns the invariants that are about the *set* of
rules rather than any rule: ids are unique, ids are stable, and the total is
capped at 30 for v0.1. `Registry::len()` spans both lists, so "30 rules" stays
one number.

### What "not a post-pass" means here

T2.2 forbids a **second pass over page bodies**, not all work after the last
fetch. `SiteRule`s are `GROUP BY`/join queries over indexed columns —
milliseconds on 500k rows, touching no body text and allocating nothing
proportional to crawl size. Per-page rules stream during the crawl as
specified.

## 3. Where each awkward rule lands

**The 11 cross-page rules are `SiteRule`s.** `duplicate title` is
`GROUP BY title HAVING count(*) > 1`. `broken internal link` joins `links` to
`pages` on `target_url`. `orphan page` is pages with no inbound edge. All are
indexed queries.

**The 2 image rules are also `SiteRule`s** — no third shape. Image checking is
*crawl* work, not rule work: the crawler issues `HEAD` requests for `<img src>`
targets and stores status and `Content-Length` in a `resources` table. The
rules then join against it like any other cross-page rule.

### New crawl capability: image HEAD requests

Deliberately scoped so it cannot harm the thing that already works:

- **`HEAD` only.** Status and `Content-Length`; no bodies, so no memory growth
  and a fraction of the bytes.
- **Its own concurrency budget**, separate from page fetching, so images can
  never starve the crawl of the pages a user actually asked for.
- **Skippable** with a flag, and subject to the same robots.txt and per-host
  rate limits as everything else. Politeness defaults are correctness.
- **Cost, stated up front:** the fixture averages ~4 images per page, so this
  can multiply request count several times over. It must be benchmarked
  separately, and page-crawl throughput must be shown not to regress.

Results land in one table, keyed by URL because a resource is referenced from
many pages and fetched once:

```sql
CREATE TABLE resources (
    url            TEXT    PRIMARY KEY,
    status         INTEGER NOT NULL,
    content_length INTEGER,          -- NULL when the server declared none
    content_type   TEXT
) STRICT;
```

`content_length` is nullable rather than zero because absent and empty are
different findings here as everywhere else: a server that declares no length is
a different report from one that declares zero bytes. `oversized image` must
therefore treat NULL as *unknown*, not as *small*.

## 4. Two `PageRecord` additions

Both are computed inside the existing single extraction pass. Neither adds a
traversal or holds memory proportional to page size.

- **`title_count: u16`** — `multiple <title>` cannot fire today: the extractor
  takes the first `<title>` and silently discards the rest, recording nothing.
- **`body_hash: Option<u64>`** — `duplicate body` needs comparable content
  without retaining it. Hashed incrementally as text chunks stream past the
  word counter, over the same whitespace-collapsed text the count sees, so
  memory stays constant and two pages differing only in markup still match.

## 5. Issue storage (T2.3)

```sql
CREATE TABLE issues (
    id       INTEGER PRIMARY KEY,
    page_id  INTEGER NOT NULL REFERENCES pages (id) ON DELETE CASCADE,
    rule_id  TEXT    NOT NULL,
    severity TEXT    NOT NULL,
    detail   TEXT
) STRICT;
```

**`severity` is denormalised into the row on purpose.** The grid filters
millions of issues by severity, and a join to rule metadata per row is the
query pattern M3 exists to avoid. It also makes a `.pounce` file self-contained:
it records what was found *at crawl time*, and a later severity re-grading in
code does not silently rewrite history.

**Indices are built by `build_query_indices()`, not the migration** —
`issues_page`, `issues_rule`, `issues_severity` — following the rule the 500k
measurement taught: an index nothing reads during a crawl is not maintained
during one. These are cheap by comparison (30 rule ids, 3 severities, so the
B-trees are shallow and stay cached), but the principle is uniform and the cost
is zero.

**Issues are written in the same transaction as their page.** `Writer::push`
returns the page's `id`, and `Writer::issues(page_id, &[Issue])` appends within
the open batch. A page and its findings therefore commit or roll back together;
there is no state where a page exists with half its issues.

## 6. Execution

Per-page rules run **after parse, before the writer** — the pipeline stage
boundary where the `PageRecord` exists and the page id does not yet matter.
Issues travel with the record into the writer stage.

Site rules run **after the crawl loop, before `build_query_indices()`**, so
they can use the crawl-time indices and their own output is indexed with
everything else.

## 7. Testing

- **Every rule ships two fixtures**, one triggering and one not. Non-negotiable
  and already an invariant; it is what stops rule count becoming rule debt.
- **`PageRule` tests need no server and no database** — a `PageRecord` literal
  in, an assertion on the issues out. This is the main reason the trait is
  shaped this way.
- **`SiteRule` tests seed a store** with a handful of pages and assert the
  issues found. A shared `seed(&[...])` helper keeps each test to a few lines.
- **Gate M2's 10% budget gets a criterion bench**: the same crawl with rules
  enabled and disabled. It is a gate with a number, so it needs a measurement,
  not an assurance.
- **Registry tests**: ids unique, ids match `^[a-z]+\.[a-z0-9-]+$`, total ≤ 30.

## 8. Prerequisite work, to land before the rule batches

- **The fixture site emits no external links and no `rel="nofollow"`.** Flagged
  under T1.2. `broken external link` and `orphan page` need them, and adding
  them **changes rendered page bytes, so the parse bench and the crawl
  baselines must be re-taken.** Doing this first means re-taking once rather
  than discovering it during batch four.

## 9. Deliberately not in this design

- **No rule configuration.** Thresholds (title length, thin-content word count)
  are constants in v0.1. Configurable thresholds are a `pounce.toml` concern
  (T6.1) and adding them now multiplies every rule's test matrix.
- **No rule ordering or dependencies.** Rules are independent; if one ever
  needs another's output, that is a signal to merge them.
- **No `Pass` issues stored.** The spec's `Pass` severity is a UI state for
  "checked, nothing found". Writing a row per rule per page is 15M rows at 500k
  to record absence. Passes are derived: rules that produced no issue passed.
- **No stable ABI.** M10's Rule SDK will need one; designing for it now would
  freeze an interface before 30 rules have tested it.
