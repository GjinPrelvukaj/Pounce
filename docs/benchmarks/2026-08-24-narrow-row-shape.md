# T3.0 — the narrow-row shape, decided by measurement

**Date:** 2026-08-24
**Host:** Apple M5, macOS 27.0. Dev laptop, not a controlled rig.
**Harness:** `crates/pounce-store/tests/narrow_row_shape.rs`, both arms
hand-rolled over the production pragmas and statements so the only difference
between them is the schema.

```bash
cargo test --release -p pounce-store --test narrow_row_shape -- --ignored --nocapture
```

---

## 1. The question

`docs/plans/2026-08-23-m3-query-layer.md` §2 left one fork open, and everything
after it depends on the answer:

- **Wide** — keep today's `pages` and add the probe's duplicated `row_view`
  table, built after the crawl.
- **Split** — `pages` becomes the narrow grid row; its six repeating JSON
  columns (`redirect_chain`, `h1`, `h2`, `hreflang`, `open_graph`, `images`)
  move to a `page_detail` table keyed by the same id.

Split's only known cost was **one extra insert per page on the crawl's hot
path**, and this repo's history says write-path changes at scale are where the
surprises live. The plan's rule was: if that insert costs more than ~2% of
throughput, take `row_view` and accept the duplication.

## 2. Write path — 100k pages, medians of 5 interleaved pairs

Records built once outside the timed section; each arm writes them through the
same structure as `writer::push` — one transaction per 500 rows, the upsert,
the `SELECT id` after it, and 28 links per page deleted and reinserted. Arm
order alternates between pairs so warm-up or throttling cannot favour whichever
arm always runs first. Both arms assert 100,000 page rows and 2,800,000 link
rows before their timing is used; split additionally asserts one `page_detail`
row per page.

| pair | wide | split |
|---|---:|---:|
| 0 | 3.767 s | 3.698 s |
| 1 | 3.634 s | 3.636 s |
| 2 | 3.740 s | 3.582 s |
| 3 | 3.763 s | 3.664 s |
| 4 | 4.016 s | 3.702 s |
| **median** | **3.763 s** (26,571 pages/s) | **3.664 s** (27,294 pages/s) |

**Split is 2.65% _faster_, not slower.** The extra insert is real, but the rows
it is added to are narrower — six JSON columns' worth of bytes leave the `pages`
B-tree, which carries eight indices, and land in a table that carries none. The
saving is larger than the cost. Split wins in four of the five pairs and ties in
the fifth.

## 3. Grid query — 1M rows, offset 500,000

The probe's worst case, unchanged: an unselective filter and a sort on a
different column, paged into the middle. `WHERE status = 200 ORDER BY
word_count, id LIMIT 200 OFFSET 500000`, best of three, both shapes carrying a
composite `(status, word_count)` index on whatever table the grid reads.

Every row is a 200, as in the probe — that is what makes the filter maximally
unselective, and it is deliberately the pessimistic case. Links are not seeded:
the grid query never reads them, and at 1M they are 28M rows that change no
measured number.

| | wide (`row_view`) | split (`pages`) |
|---|---:|---:|
| Grid query | **7.25 ms** | **7.71 ms** |
| Query plan | `SEARCH row_view USING INDEX grid_status_word` | `SEARCH pages USING INDEX grid_status_word` |
| Post-crawl build | 1.19 s (copy + index) | 0.32 s (index only) |
| File | 979 MB | **873 MB** |

Both are two orders of magnitude inside the 300 ms gate, and neither plan uses
a temp B-tree — the test asserts that rather than trusting it. The two shapes
are asserted to return the **same 200 row ids**, so this is one query measured
twice, not two queries.

The 106 MB gap is the duplication: at 1M rows, `row_view` is a second copy of
every grid column.

**A detail that cost an assertion failure and is worth keeping:** `CREATE TABLE
row_view AS SELECT ...` produces a table whose `id` is an ordinary column, not
the rowid. The composite index's implicit tail is then not the id, and the
`, id` tie-break falls back to `USE TEMP B-TREE FOR LAST TERM OF ORDER BY`.
`row_view` has to be declared with `id INTEGER PRIMARY KEY` and filled by
`INSERT ... SELECT`. Any future materialised copy of `pages` has the same trap.

## 4. Verdict — split `pages`, and `row_view` is the loser

Split wins on every axis measured:

| | wide | split |
|---|---|---|
| Write throughput | baseline | **2.65% faster** |
| Grid query at 1M | 7.25 ms | 7.71 ms (both ≪ 300 ms) |
| File at 1M | 979 MB | **873 MB** |
| Post-crawl build | 1.19 s at 1M, ~17 s at 500k in the probe | 0.32 s |
| Duplication | every grid column twice | none |
| Two copies of `status` can disagree | yes | no |

The one number that could have gone the other way — the extra insert — went the
right way, so the duplication buys nothing. **`row_view` is not adopted.**

The probe's 4.2 GB link graph is absent from both arms of §3, so the file sizes
compare `pages` and its dependents only; that is the part the shapes differ in.

## 5. What this does not establish

- **Store write path, not whole-crawl throughput.** Fetch and parse are
  identical between arms, so the 2.65% is a floor for the crawl-level figure,
  not the crawl-level figure itself.
- **One filter/sort pair.** `status`/`word_count` again, still the worst case by
  construction. T3.2 measures selectivity across the rest.
- **Synthetic rows, all 200s.** Realistic status mix arrives with T3.5's seeder.
- **No concurrent writer.** WAL readers under a live writer remain untested,
  as they were in the probe.
