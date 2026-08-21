# Fixing the near-quadratic scaling — what worked and what backfired

**Date:** 2026-08-21
**Host:** Apple M5, macOS 27.0, rustc 1.97.1, `--release`. Dev laptop, not a controlled rig.
**Command:** `bench-runner --pages {10000,100000,500000} --seed 42 --tool 'pounce=…'`
**Baseline:** [`2026-08-21-pounce-scale-100k-500k.md`](2026-08-21-pounce-scale-100k-500k.md)

---

## 1. Result

| Fixture | Wall before | Wall after | Speedup | RSS before | RSS after |
|---|---:|---:|---:|---:|---:|
| 10k | 2.71 s | **1.9 s** | 1.43× | 28 MB | **24 MB** |
| 100k | 66.1 s | **19.3 s** | 3.43× | 64 MB | **72 MB** |
| 500k | 1,391.5 s | **142.6 s** | **9.76×** | 228 MB | **257 MB** |

**Scaling 100k → 500k: 5× the pages for 7.39× the wall time**, against 21.1×
before. Linear would be 5×. **Memory is essentially unchanged** — the small rise
is the end-of-crawl index build.

Integrity verified on every run at 500k: 500,001 pages, 500,001 distinct, 0
frontier entries pending, 13,999,791 links, `links_target` present.

## 2. What worked — two structural changes

**Defer `links_target` to the end of the crawl** (migration 007 +
`Store::build_query_indices`). Nothing reads `target_url` while crawling; the
frontier owns dedup and the index exists only for the detail pane's inlinks
query. Maintaining it meant 14M random inserts into a TEXT B-tree; building it
once is a single sorted pass.

*Measured in isolation at 500k:* **1,391.5 s → 376.3 s (3.70×).** This is also
the experiment that settled the diagnosis — the `frontier`'s `WITHOUT ROWID`
TEXT primary key is **not** the dominant cost, so the expensive redesign it
would have needed is not required.

**Dedupe before persisting.** The CLI called `writer.discover()` on every
extracted link and *then* pushed to the in-memory frontier. On this graph ~28
links per page resolve to about one new URL, so ~14M upserts happened where
~500k were needed. `Frontier::push` already returns `PushResult`, so filtering
on it is a reorder, not a new mechanism.

*Measured:* the remaining **376.3 s → 142.6 s (2.64×)**.

## 3. What backfired — both PRAGMA changes

Tried on the reasonable theory that a 2 MB page cache against a 4.7 GB database
must be thrashing. Both were worse, and both are now asserted against by tests.

| 500k configuration | Wall | Peak RSS | Verdict |
|---|---:|---:|---|
| **defaults** | **142.6 s** | **257 MB** | kept |
| `cache_size = -65536` (64 MB) | 152.8 s | 356 MB | slower **and** 99 MB heavier |
| `+ temp_store = MEMORY` | 141.9 s | **1,592 MB** | **failed the 400 MB gate** |

**`temp_store = MEMORY` is the instructive one.** It is harmless on its own and
harmless before this work. It became catastrophic *because* of the fix in §2:
deferring the index turns it into a 14M-row external sort, and the pragma tells
SQLite to hold that sort in RAM. It bought ~10 s of a 142 s crawl for 1.3 GB.

**The page cache buys nothing here.** At 100k it was 19.6 s / 200 MB against
19.3 s / 72 MB at the default — no measurable speed, 128 MB spent. Once
`links_target` is deferred the write path is append-mostly, so a larger cache
holds pages nothing reads again while competing with the OS page cache that was
already doing the job.

**Why this was nearly missed.** Wall time alone called the 1,592 MB run the best
result of the day. Only the RSS gate — a number with a threshold attached —
exposed it. A throughput-only benchmark would have shipped it.

## 4. Consequences elsewhere

- **The FreeCrawl head-to-head improves and should be re-taken.**
  [`2026-08-21-freecrawl-head-to-head-10k.md`](2026-08-21-freecrawl-head-to-head-10k.md)
  measured Pounce at 2.71 s / 28 MB. It is now 1.9 s / 24 MB, and the gap widens
  with scale rather than narrowing. **Those figures are stale, not wrong.**
- **M9's 10M-URL gate is no longer obviously unreachable.** The old curve
  extrapolated to 40–150 hours. At 7.39× per 5× it is a few hours — still an
  extrapolation, and 10M has never been run.
- **Gate M1's 500k RSS item still passes**, now at 257 MB against 400 MB.

## 5. What is still not established

- **Single run per configuration at 500k.** No medians, no spread.
- **Still super-linear.** 7.39× wall for 5× pages, not 5×. Something continues
  to cost more as the crawl grows — the `pages.url` unique index and the
  `frontier` TEXT primary key are the remaining candidates, unmeasured.
- **Not profiled.** Every attribution here is a controlled A/B on wall time and
  RSS, not a flame graph.
- **Localhost.** A polite crawl against a real site is bounded by the target
  server long before any of this matters.
