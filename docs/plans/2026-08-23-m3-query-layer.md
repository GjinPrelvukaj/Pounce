# M3 — Query layer: implementation plan

**Date:** 2026-08-23
**Status:** written on reaching M3, per `PLAN.md`'s standing rule.
**Reads:** [`docs/benchmarks/2026-08-22-m3-query-gate-probe.md`](../benchmarks/2026-08-22-m3-query-gate-probe.md)
— the probe that says the architecture holds and the schema does not.

---

## 1. What the probe already decided, and what it left open

The probe measured the worst-case grid query at **18,270 ms against a 300 ms
gate**. It is not a mystery to be investigated; it is a known cause with two
measured fixes that compose to 10 ms. This plan is about landing them, not
rediscovering them.

**Decided before this plan (owner's call, 2026-08-23): `OFFSET` stays.**
Keyset pagination was priced and rejected. It removes the deep-skip cost with
single-column indices, but it cannot answer "jump to row 500,000" without
counting, so the virtualised grid's scrollbar degrades from scrubbing to
next/prev paging. The spec's *scroll position maps to `OFFSET`* is load-bearing
for the product's central interaction, so it stands and the schema bends
instead. Recorded here so a later reader sees a decision rather than an
oversight.

**Left open, and this plan resolves each by measurement, not argument:**

- Which narrow-row shape: a duplicated `row_view` table, or splitting `pages`.
- Which filter × sort pairs get composite indices. The full cross product is
  ~54 and is not an option.
- Whether the index build cost is acceptable against M1's throughput figures.

## 2. The one design fork worth resolving first (T3.0)

The probe measured a **`row_view` table** — `id, url, status, depth, size,
word_count, title, kind, noindex`, 83 MB at 500k, built post-crawl in ~17 s.
It works. But it duplicates every column it names, and duplication in a file
format is a correctness surface: two copies of `status` can disagree.

**The alternative is to split `pages` rather than copy it.** `pages` becomes
the narrow grid row; the wide parts it already stores as JSON — `h1`, `h2`,
`hreflang`, `open_graph`, `images`, `redirect_chain` — move to a `page_detail`
table keyed by the same id. Same narrow-row benefit, no duplication, no
post-crawl build step, and the detail pane already fetches one row at a time so
its join costs nothing that matters.

**The catch, and why this is measured rather than asserted:** it adds a second
insert per page on the crawl's hot path. M1's history says write-path changes
at scale are exactly where this project's surprises live — deferring one index
was worth 9.76x — so a second table must be shown not to cost throughput before
it is adopted.

| | `row_view` (probe's shape) | split `pages` / `page_detail` |
|---|---|---|
| Duplication | every grid column twice | none |
| Post-crawl build | ~17 s at 500k | none |
| Crawl write cost | unchanged | **one extra insert per page — measure** |
| Disagreement possible | yes | no |
| Migration | additive | rewrites `pages` |

**T3.0 decides this with a measurement**, and everything after depends on the
answer. If the second insert costs more than ~2% of crawl throughput, take
`row_view` and accept the duplication with a comment saying why.

### Decided 2026-08-24 by measurement: **split `pages`**. `row_view` is the loser.

[`docs/benchmarks/2026-08-24-narrow-row-shape.md`](../benchmarks/2026-08-24-narrow-row-shape.md)
— harness `crates/pounce-store/tests/narrow_row_shape.rs`, 100k interleaved
pairs for the write path, 1M for the query.

| | wide (`row_view`) | split (`pages` + `page_detail`) |
|---|---:|---:|
| Write, 100k, median of 5 | 3.763 s (26,571 pages/s) | **3.664 s (27,294 pages/s)** |
| Grid query, 1M, offset 500k | 7.25 ms | 7.71 ms |
| File, 1M | 979 MB | **873 MB** |
| Post-crawl build, 1M | 1.19 s | 0.32 s |

The extra insert was the whole risk and it **did not cost throughput** — split
came out 2.65% faster, because the six JSON columns leave a `pages` B-tree
carrying eight indices and land in a table carrying none. Both shapes answer the
worst query in ~7 ms against a 300 ms gate, returning an identical 200 row ids,
neither using a temp B-tree. So the duplication buys nothing and is not taken.

**Trap found on the way, kept because any future materialised copy hits it:**
`CREATE TABLE row_view AS SELECT ...` gives `id` as an ordinary column rather
than the rowid, and the `, id` tie-break then costs `USE TEMP B-TREE FOR LAST
TERM OF ORDER BY`. It has to be `id INTEGER PRIMARY KEY` plus `INSERT ... SELECT`.

**What this makes T3.1–T3.3 inherit:** `pages` is migrated to the narrow shape
with `page_detail` alongside it. No site rule reads any of the six moved columns
in SQL — they are read off `PageRecord` — so the blast radius is one migration
and `writer::push`.

## 3. Tasks

### T3.0 — Narrow-row shape, decided by measurement *(new)*

Build both shapes behind the same query, run the A/B that T2.9a and the M2
re-take established as this repo's pattern — interleaved pairs, medians, both
arms asserted to produce identical row counts — at 100k pages.

- Crawl throughput, split vs current schema.
- Grid query latency, both shapes, at 1M.
- File size, both shapes.

Write the loser into the plan with its numbers. **Do not skip to the winner.**

### T3.1 — `FilterSpec` → parameterised `WHERE`

A closed enum, not a string. The spec says "no string interpolation" and the
reason is that this compiles user input into SQL.

```rust
pub enum Filter {
    Status(Comparison, u16),
    Depth(Comparison, u16),
    WordCount(Comparison, u32),
    Kind(BodyKind),
    Noindex(bool),
    HasIssue(Option<&'static str>),   // any issue, or one rule id
    UrlContains(String),              // the only free-text case
}
```

Every variant compiles to a fragment with **bound parameters only**. `UrlContains`
binds a `LIKE` pattern; it never concatenates. Rule ids are `&'static str` from
the registry, so `HasIssue` cannot name a rule that does not exist.

**Tests:** one per variant, both matching and not; a test that a filter carrying
`'; DROP TABLE pages; --` returns zero rows and leaves the table intact; and a
test that the compiled SQL contains no user bytes, asserted against the
statement's parameter count rather than by eyeballing the string.

### T3.2 — `SortSpec` restricted to indexed **combinations**

The probe invalidated this task as written. "Indexed columns" is not enough: an
indexed sort column is still 18 s when the active filter is a different indexed
column matching most rows.

```rust
/// A filter shape and a sort column that are known to be backed together.
pub struct SupportedPair { filter: FilterKind, sort: SortColumn }
```

`SortSpec::new(filter, sort)` returns `Err` for an unsupported pair. The
supported set is **declared in one place**, and a test asserts that every
declared pair has a matching index *and that the query plan for it does not say
`USE TEMP B-TREE`*. That assertion is the whole point — it is the thing that
was missing when `links.orphan-page` silently cost 45 seconds.

**Which pairs**: chosen from measured selectivity, not taste. A filter that
matches few rows needs no composite index; only unselective ones do. T3.2
begins by counting, on a real crawl, how selective each filter actually is, and
declares composites only where the count is high. Expected to be far fewer than
54 — but the number comes from the data.

### T3.3 — Windowed `query_rows(offset, limit)`

Returns `Page<RowView>` — the visible window and a total count, never full
records. `limit` is clamped server-side; a caller asking for 100,000 rows gets
the clamp, because the invariant is "the UI never receives the dataset" and an
unclamped limit is that invariant resting on the caller's manners.

**Tests:** the window matches a hand-computed slice; memory is flat across a
200x change in result size (the probe measured 11 MB → 12 MB with the CLI, and
this must hold through the real layer); a query on a live WAL reader under a
concurrent writer returns without blocking.

### T3.4 — Aggregate queries for the issue overview

`GROUP BY rule_id, severity` over `issues`, plus totals. The indices exist
already (`issues_rule`, `issues_severity`, built by `build_query_indices`).
Cheap, and mostly a matter of not accidentally joining `pages`.

### T3.5 — A 1M-row database, and the gate

**The probe's databases no longer exist**, and re-crawling 1M pages to get one
back is hours. T3.5 builds a **seeder** instead — the same shape the M2
site-rule harness uses, which produced 500k stores in minutes — with a realistic
status mix rather than the all-200 fixture, because all-200 is what made the
probe's filter maximally unselective.

Then re-run every Gate M3 item through the **real query layer**, not the
`sqlite3` CLI, and publish. The probe's own list of what it did not establish
is the checklist: prepared statements, connection reuse, a live writer, and
more than one filter/sort pair.

## 4. What this plan deliberately does not do

- **No UI.** M3 exists to prove the claim before any UI is built on it.
- **No `ANALYZE`.** Measured at 16 s; it swaps a temp B-tree for a per-row table
  lookup and is not a fix.
- **No relaxing of the gate.** If the numbers cannot be hit the fix is indices
  or schema — never loading more into the UI.

## 5. Risks

**The composite index set grows without anyone noticing.** Each pair is cheap
alone; twenty are 239 MB at 1M. The declared-pairs list is the control, and a
test should assert the total index count against a stated ceiling so adding the
twenty-first is a decision.

**The seeder's data shape decides the answer.** An unrealistic status mix makes
the worst query either trivially fast or artificially slow. The seeder's
distribution has to be stated in the benchmark doc, and the pessimistic case
kept as one of the measured pairs.

---

## 6. Landed

- **T3.0** (2026-08-24) — split `pages`, `row_view` rejected; migration 012.
  [`docs/benchmarks/2026-08-24-narrow-row-shape.md`](../benchmarks/2026-08-24-narrow-row-shape.md)
- **T3.1** — `Filter`/`FilterSpec` in `pounce-store/src/query.rs`, parameterised
  only, with the injection case and a parameter-count assertion.
- **T3.2** — 20 composites declared from measured selectivity; the 7-index
  sort-first alternative priced and refuted.
  [`docs/benchmarks/2026-08-24-filter-sort-pairs.md`](../benchmarks/2026-08-24-filter-sort-pairs.md)
- **T3.3** — `query_rows` returning `Page<RowView>`, limit clamped to
  `MAX_WINDOW`, tested against a live WAL reader under an open write batch.
- **T3.4** — `issue_overview()` off `issues` alone; counts issues and distinct
  URLs, and keeps the findings whose subject never became a page.

- **T3.5** — the 1M seeder and Gate M3, **passed 2026-08-24**. Worst supported
  pair 220 ms against a 300 ms gate, sorts 6.5–11.5 ms against 150 ms, memory
  12 MB → 12 MB for a 200x window, a live reader at 13.3 ms with a batch open.
  [`docs/benchmarks/2026-08-24-gate-m3.md`](../benchmarks/2026-08-24-gate-m3.md)

The 1M run is what corrected T3.2: at 200k, a range filter and an equality
filter on the same column looked alike. They do not scale alike, and the support
rule now keys on the *shape* of the comparison.
