# Store insert throughput — batch size

**Date:** 2026-08-21
**Host:** Apple M5, macOS 27.0, rustc 1.97.1, `--release` (`profile.bench` inherits release)
**Command:** `cargo bench -p pounce-bench --bench store`
**Fixture:** 5,000 pages from `SiteGraph::generate(seed 42)`, rendered and run
through `parse_body` **before** the timed section, so this measures the writer
and its indices, not parsing.
**Database:** a real file in a fresh temp dir per iteration, WAL,
`synchronous = NORMAL`. Not `:memory:` — an in-memory number would be
flattering and would say nothing about what throttles a crawl.

## Result

| Batch size | Time for 5,000 rows | Throughput |
|---|---|---|
| 1 | 314.03 ms | **15.9k rows/s** |
| 100 | 69.00 ms | **72.5k rows/s** |
| **500** | **52.23 ms** | **95.7k rows/s** |
| 2,000 | 46.88 ms | **106.7k rows/s** |

Medians of criterion's 10-sample estimate; the confidence intervals are within
±1% of each estimate except batch/1, which spans ±0.8%.

## What it says

**~500 was a good guess.** It captures 6× the per-row rate. Going to 2,000
adds 11% and quadruples the window of rows lost to an interrupted crawl, which
is not a trade worth making for a writer that is already not the bottleneck.

**The writer is not the bottleneck, and that is the load-bearing conclusion.**
The fixture site's own serving ceiling is ~17,000 req/s
([2026-08-20](2026-08-20-fixture-ceiling-and-freecrawl-probe.md)), so at batch
500 the store absorbs rows about **5.6× faster than the harness can serve
them**. Any throughput number this project publishes will be bounded by the
network and the target server, not by SQLite.

## Caveats — do not quote these figures without them

- **One platform.** Apple M5 with fast NVMe. `synchronous = NORMAL` under WAL
  means batch/1 is *not* paying an fsync per row, so on a slower disk the gap
  between batch sizes would widen, not narrow. The 500-vs-2000 conclusion is
  the one most likely to change on other hardware.
- **Empty database.** Each iteration inserts into a fresh file, so every insert
  appends to indices that never exceed 5,000 entries. A 1M-row crawl pays more
  per row as the eight indices deepen. **This is not a 1M-row number**, and
  Gate M3's 500k/1M seeded database is where that gets measured.
- **No concurrent readers.** A real crawl has the UI querying while this
  writes. WAL is what makes that survivable, and it is untested here.
- **No link rows.** T1.14 adds the edge table, whose insert volume is roughly
  one row per link — an order of magnitude more rows than pages. That will
  dominate, and this figure will need re-taking once it exists.
