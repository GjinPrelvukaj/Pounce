# Gate M3 probe — does query-don't-dump hold?

**Date:** 2026-08-22
**Host:** Apple M5, macOS 27.0, `sqlite3` CLI against real `.pounce` files. Dev laptop, not a controlled rig.
**Databases:** real crawls of the fixture — 500,001 pages / 4.7 GB, and
1,000,001 pages / ~9 GB. Not synthetic seed data.
**Status:** a **probe**, run before M3 is implemented. No production code changed.

---

## 1. Why this was run early

`PLAN.md` puts M3 after M2 and calls it *"prove the load-bearing architectural
claim before a single line of UI"*. The spec names query-don't-dump as **the
single most likely way the project fails**.

The gate was expensive to test when the plan was written. It is cheap now,
because M1's benchmarking left real 500k and 1M databases behind. Testing it
before writing 30 audit rules and 15 GUI tasks costs an hour; testing it after
costs those tasks.

## 2. Result — the gate fails as the schema stands

| Gate item | Target | Measured | |
|---|---:|---:|:--|
| Sort of 500k rows | < 150 ms | **0–10 ms** | ✅ |
| Filter + sort + paginate over 1M | < 300 ms | **18,270 ms** | ❌ **61× over** |
| Memory flat regardless of result size | — | 11 MB → 12 MB for a 200× larger result | ✅ |

Everything the grid does *except one shape* is comfortably fast. Sorting a
million rows by any indexed column and paging to the middle of it is 10 ms.

**The failure is filter and sort on different columns when the filter is
unselective.** `WHERE status=200 ORDER BY word_count` matches every row in a
healthy crawl, so SQLite filters on `pages_status` and then builds a temp
B-tree over half a million rows to sort them.

## 3. Diagnosis

`EXPLAIN QUERY PLAN` shows `USE TEMP B-TREE FOR ORDER BY`. Two things make it
expensive, and only the second is fixable by indexing alone:

- **Row width.** `pages` is 476 MB at 500k. The sort must touch all of it.
  (The database's other 4.2 GB is the link graph, which this query never reads.)
- **No index serving both predicates.** One index can satisfy the filter or the
  order, not both.

`ANALYZE` was tried and **does not fix it**: it changes the plan to walk
`pages_word_count` instead, avoiding the temp B-tree, but then pays a table
lookup per row to test `status` — 16 s, no better.

## 4. Two fixes, measured, and they compose

| At 1M rows | filter+sort+paginate |
|---|---:|
| As-is | 18,270 ms |
| Narrow `row_view` projection | **500 ms** |
| `row_view` + composite `(status, word_count)` | **10 ms** |

**The narrow projection alone is not enough.** 500 ms still misses the 300 ms
gate at 1M. It is a 36× improvement and it is not sufficient on its own — worth
stating plainly, because 500 ms looks like success next to 18 s.

`row_view` is `id, url, status, depth, size, word_count, title, kind, noindex`
— 83 MB at 500k, 239 MB with indices at 1M, built post-crawl in ~17 s. T3.3
already calls for a `RowView` projection; this says it must be a **table**, not
just a return type.

## 5. What this changes about M3's design

**T3.2 says "`SortSpec` restricted to indexed columns". That is not strong
enough.** The measurement says: restricted to indexed **combinations**. A sort
column that is indexed is still 18 s if the active filter is on a different
indexed column and matches most rows.

So the UI cannot offer arbitrary filter × sort. It must offer a **declared,
bounded set of pairs**, each backed by a composite index built after the crawl.
Which pairs, and how many indices that implies, is M3 design work — with ~6
filterable and ~9 sortable columns, the full cross product is 54 indices and is
not an option.

**An alternative worth pricing before committing:** keyset pagination
(`WHERE word_count > :last`) removes the deep-skip cost entirely and needs only
single-column indices. It conflicts with the documented invariant that *scroll
position maps to `OFFSET`*, so it would require changing the spec first — which
is the correct order, not a reason to dismiss it.

## 6. Verdict

**The architecture holds. The schema does not, yet.**

Nothing here suggests loading more into the UI, and the plan's own instruction —
*"the fix is indices or schema, never loading more into the UI"* — is exactly
what the measurements point at. Two schema fixes are already measured to work,
and together they land the worst query at 10 ms against a 300 ms budget.

**Gate M3 is not passed and must not be ticked.** M3 is unimplemented; this
probe only establishes that it is implementable and what it has to do.

## 7. What is not established

- **`sqlite3` CLI, not the real query layer.** Prepared statements, connection
  reuse and WAL readers under a live writer are all untested.
- **No concurrent writer.** A real crawl queries while it writes; WAL is meant
  to make that survivable and this did not test it.
- **Best-of-3 with 10 ms timer resolution.** Anything reported as 10 ms could be
  anywhere below it; the 18 s and 500 ms figures are unambiguous.
- **One filter/sort pair.** `status`/`word_count` is the worst case by design
  because the filter matches everything. Other pairs will be cheaper; none was
  measured.
- **Fixture data.** Every page is a 200, which is *why* the filter is
  unselective. A real crawl has a status mix, so this is a pessimistic case —
  deliberately.
