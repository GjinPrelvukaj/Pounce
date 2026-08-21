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

## T1.13 result — page rows only (superseded workload)

| Batch size | Time for 5,000 rows | Throughput |
|---|---|---|
| 1 | 314.03 ms | **15.9k rows/s** |
| 100 | 69.00 ms | **72.5k rows/s** |
| **500** | **52.23 ms** | **95.7k rows/s** |
| 2,000 | 46.88 ms | **106.7k rows/s** |

These are medians of criterion's 10-sample estimate. They remain the cost of
writing page rows alone, but no longer describe a crawl now that T1.14 writes
the extracted link graph in the same transaction.

## T1.14 re-take — pages plus link graph

The same 5,000 pages contain **104,807 links** (21.0/page), so each iteration
now writes 109,807 SQL rows across `pages` and `links`.

| Batch size | Time for 5,000 pages | Pages/s | Total SQL rows/s |
|---|---|---|---|
| 1 | 4.7981 s | **1.04k** | 22.9k |
| 100 | 1.9915 s | **2.51k** | 55.1k |
| **500** | **1.1573 s** | **4.32k** | **94.9k** |
| 2,000 | 947.09 ms | **5.28k** | 115.9k |

These are the medians from the final full run of
`cargo bench -p pounce-bench --bench store` after T1.14.

## T1.15 re-take — durable frontier present

T1.15 corrected the benchmark setup to seed all 5,000 URLs into the durable
frontier before the timed section. Completion is derived by joining frontier
URLs to durable page rows, so the timed writer does not maintain a duplicate
completion flag.

| Batch size | Time for 5,000 pages | Pages/s | Total SQL rows/s |
|---|---|---|---|
| 1 | 4.6931 s | **1.07k** | 23.4k |
| 100 | 1.9425 s | **2.57k** | 56.5k |
| **500** | **1.1238 s** | **4.45k** | **97.7k** |
| 2,000 | 933.52 ms | **5.36k** | 117.6k |

These are medians from the final full run of the same command after T1.15.
Batch 500 changed from 4.32k to 4.45k pages/s, inside the variability already
observed during T1.14: **the durable frontier adds no measured hot-path cost.**

**The old bottleneck conclusion is superseded.** Batch 500 still sustains about
the same total row rate as before (97.7k versus 95.7k rows/s), but a fixture
page expands to about 22 SQL rows. At 4.45k pages/s the writer is below the
fixture server's ~17k req/s ceiling, so SQLite now bounds an unrestricted local
crawl. This is the real workload and the Gate M1 crawl must carry that cost.

**500 remains the default.** It is 4.2× batch/1. Batch 2,000 buys another 20%,
but makes a single-host crawl keep four times as many fetched pages uncommitted;
under polite network rates that durability window is measured in minutes, not
the sub-second transaction time shown here.

## Caveats — do not quote these figures without them

- **One platform.** Apple M5 with fast NVMe. `synchronous = NORMAL` under WAL
  means batch/1 is *not* paying an fsync per row, so on a slower disk the gap
  between batch sizes would widen, not narrow. The 500-vs-2000 conclusion is
  the one most likely to change on other hardware.
- **The machine was noisy during the re-take.** Repeated batch/500 medians in
  the same session ranged from 4.32k to 7.59k pages/s. The tables record full
  runs, not the best run. Re-measure under controlled conditions before
  publishing a cross-tool result.
- **Empty database.** Each iteration inserts into a fresh file, so every insert
  appends to page indices that never exceed 5,000 entries and link indices that
  never exceed 104,807 entries. A 1M-page crawl pays more per row as those
  indices deepen. **This is not a 1M-page number**, and Gate M3's seeded
  database is where that gets measured. **Measured 2026-08-21 at 500k:**
  10.4k rows/s, a 9× drop, in
  [`2026-08-21-pounce-scale-100k-500k.md`](2026-08-21-pounce-scale-100k-500k.md).
- **No concurrent readers.** A real crawl has the UI querying while this
  writes. WAL is what makes that survivable, and it is untested here.
