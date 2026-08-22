# Image `HEAD` pass — does checking images cost the page crawl anything?

**Date:** 2026-08-22
**Host:** Apple M5, macOS 27.0, rustc 1.97.1, `--release`. Dev laptop, not a controlled rig.
**Fixture:** `cargo run --release -p pounce-bench --bin fixture-site -- --pages 10000 --seed 42 --port 8141`
**Commands:**

```
./target/release/pounce crawl http://127.0.0.1:8141/ --output n.pounce --quiet
./target/release/pounce crawl http://127.0.0.1:8141/ --output i.pounce --images --quiet
```

---

## 1. The requirement

The design spec's condition for adding image checking: *page-crawl throughput
must be shown not to regress.*

## 2. Result — interleaved A/B, 10,001 pages each run

Runs alternate baseline/images within each pair, because the first unpaired
attempt showed a ~6% drift across a five-run block that had nothing to do with
the flag.

| Pair | Baseline | `--images` |
|---|---:|---:|
| 1 | 10.39 s | 10.23 s |
| 2 | 10.29 s | 10.38 s |
| 3 | 10.29 s | 10.21 s |
| 4 | 10.24 s | 10.45 s |
| **Median** | **10.29 s** | **10.31 s** |

**+0.2%, inside the ±1% spread of the baseline against itself.** Both files
contain 10,001 pages, so the two runs crawled the same thing.

## 3. Why the answer is structural, not lucky

The pass runs **after** the crawl loop, not alongside it. No image request
exists while a page request is in flight, so page-crawl throughput cannot
regress by construction — the measurement above confirms the implementation
matches the design rather than establishing the property. The cost of the
choice is that the two phases do not overlap: image checking is additive wall
time, which is what `--images` being opt-in is for.

## 4. What this does **not** measure

**The fixture site references five distinct images in total** —
`/static/img-0.jpg` through `img-4.jpg`, shared by every page — so the pass
issued **5 requests** for a 10,001-page crawl. That is a real property of
template-driven sites, and it is why the URLs are deduplicated before any
request goes out, but it means this run says nothing about a site with
per-page unique images.

Two numbers are therefore **not established**:

- **Per-image cost at scale.** Needs a fixture whose image URLs vary per page.
- **Memory under a large image set.** The in-memory `HashSet` of distinct image
  URLs held 5 entries here. Its ceiling is a site with unique images per page,
  where it grows like the frontier does.

Both are recorded in `PLAN.md` under T2.9a. No figure for either should be
quoted until they are measured.
