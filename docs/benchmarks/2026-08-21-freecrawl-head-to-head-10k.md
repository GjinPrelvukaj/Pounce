# Pounce vs FreeCrawl — 10,000-page head-to-head

**Date:** 2026-08-21
**Host:** Apple M5, macOS 27.0, rustc 1.97.1, Node v25.9.0. Dev laptop, not a controlled rig.
**Tools:** Pounce `0.0.1` (release build, T1.21) · FreeCrawl `0.9.6`
([kemalai/FreeCrawl-SEO-Tool](https://github.com/kemalai/FreeCrawl-SEO-Tool), built from source, Node CLI, no JS rendering)
**Fixture:** `bench-runner --pages 10000 --seed 42`, localhost.
**Status:** A real head-to-head, on one machine, with verified counts. **Not the
Gate M1 item** — that requires 100k. See §5.

---

> **Pounce's figures here are superseded (2026-08-21).** The scaling fix in
> [`2026-08-21-scaling-fix.md`](2026-08-21-scaling-fix.md) took 10k from
> 2.71 s / 28 MB to **1.9 s / 24 MB**, and the gap widens with scale rather than
> narrowing. FreeCrawl's numbers stand. Re-take before publishing anything.

## 1. Result

Three runs each, FreeCrawl at its best configuration (§2). Medians.

| Tool | URLs crawled | Wall (median) | **URLs/s** | **Peak RSS** | Duplicates |
|---|---:|---:|---:|---:|---:|
| **Pounce** | 10,001 | **2.71 s** | **3,690** | **28 MB** | 0 |
| FreeCrawl | 10,006 | 131.82 s | 75.9 | 720 MB | — |

Spread across the three runs: Pounce 2.66–2.95 s / 28–30 MB; FreeCrawl
125.24–152.92 s / 681–722 MB.

**Pounce is ~49× faster and uses ~26× less memory** on this fixture.

URLs/s is computed from **counts each tool reported for itself**, not from the
requested page count — see §4.

## 2. FreeCrawl was given its best configuration

Its default `--rps 20` is a politeness throttle; leaving it on would have
measured the throttle, not the crawler. It was raised to 100,000 for every run.
Concurrency was then swept, and **its best result is the one in §1**:

| `--concurrency` | Wall | URLs/s | Peak RSS | Avg response |
|---:|---:|---:|---:|---:|
| **20** (default) | 116.0 s | 86.3 | 738 MB | **28 ms** |
| 50 | 118.8 s | 84.2 | 734 MB | 59 ms |
| 100 | 118.3 s | 84.6 | 775 MB | 116 ms |

**Raising concurrency does not help it.** Throughput is flat while its own
average response time climbs 28 → 59 → 116 ms, which is internal queueing
against a fixture that serves ~17,000 req/s. This explains the 2026-08-20 probe's
1,193 ms average at `--concurrency 200`: that run was not slow because of the
fixture, it was saturating its own event loop. **The old probe's number (28
URLs/s) was its worst configuration; 86 is its best, and the conclusion is
unchanged.**

## 3. Pounce was *not* given its best configuration

`pounce crawl` currently exposes only `--output` and `--quiet`. Concurrency is
hardcoded at the `FetchConfig` default of **4 requests per host**, against
FreeCrawl's 20–100. Pounce ran handicapped and still won by ~49×.

This is a real gap, not a benchmarking convenience: **the CLI has no tuning
surface**, which is T6.1 work. Recorded here because the next benchmark cannot be
called fair in the other direction until it exists.

## 4. Corrections to the harness, made before these numbers were taken

- **`bench-runner` assumed each tool crawled `--pages` URLs** and derived URLs/s
  from that assumption for every tool. It reported `pages_crawled: 10000` for
  both tools when the truth was 10,001 and 10,006. Every figure above was
  recomputed from FreeCrawl's own `--json` `summary.total` and from
  `SELECT count(*) FROM pages` in the `.pounce` file. **The tool still needs
  fixing** — it is on the 2026-08-20 redo list and remains unticked.
- **FreeCrawl's `exit 1` is not a failure.** It sets a non-zero exit when any
  status ≥ 400 appears, which is correct CI behaviour for an SEO tool, and the
  fixture serves 4xx deliberately. The 2026-08-20 probe recorded that exit as
  "a run that did not cleanly succeed"; that reading was wrong. Its 121 *failed
  requests* were a genuine problem and are separate.
- **Counts verified, not assumed.** Pounce wrote 10,001 pages with 10,001
  distinct URLs in all three runs — no duplicates, no losses at this scale.

## 5. What this does not establish

1. **It is 10k, not 100k.** Gate M1 requires the full fixture. Nothing here says
   how either tool behaves an order of magnitude up, and FreeCrawl's 720 MB at
   10k is the more interesting number to extrapolate, not ours.
2. **Peak RSS at 500k is still unmeasured.** 28 MB at 10k does not discharge the
   under-400 MB-at-500k gate item.
3. **Screaming Frog is untested.** It is the incumbent users actually switch
   from, it is £199/yr with 500 URLs free, and a 100k head-to-head against it is
   impossible without paying. FreeCrawl is not a proxy for it.
4. **Localhost only.** No network variance — reproducible, but not conditions any
   real crawl has. On a real site both tools would be bounded by the target
   server, and the gap would narrow to the point of being invisible to a user.
   **This is the project's own Risk #1** and this benchmark does not address it.
5. **One machine, one OS, three runs.**

## 6. Decision

Gate M1's rule: *if a competitor lands within ~20% of our throughput, stop and
revisit positioning.* FreeCrawl lands at **2% of it**, with 26× the memory.

**Against FreeCrawl, the premise holds and there is no reason to change course.**
The gate is not yet passed — 100k, 500k RSS, and Screaming Frog remain — but the
"stop and revisit" condition is not triggered by this result.

The finding worth carrying into the GUI milestone is the memory one. 28 MB vs
720 MB is a difference a user feels on a laptop with other work open; 2.7 s vs
132 s on localhost is a difference that partly evaporates behind real network
latency. **Benchmark headlines should lead with peak RSS**, as the 2026-08-20
parse-throughput note already concluded for a different reason.

## Reproduce

```bash
cargo build --release -p pounce-seo -p pounce-bench

# FreeCrawl 0.9.6, CLI only (no Chromium needed for HTTP-only crawls)
git clone --depth 1 https://github.com/kemalai/FreeCrawl-SEO-Tool.git
cd FreeCrawl-SEO-Tool
PLAYWRIGHT_SKIP_BROWSER_INSTALL=1 ELECTRON_SKIP_BINARY_DOWNLOAD=1 npm install
npx tsc -b            # `npm run build:cli` alone fails: TS18046 in apps/cli/src/index.ts:526

./target/release/bench-runner --pages 10000 --seed 42 \
  --tool 'pounce=<wrapper> {url}' \
  --tool 'freecrawl=<wrapper> {url}'
```

Wrappers are needed because `pounce crawl` refuses to overwrite its output and
FreeCrawl merges into an existing project DB; each run must start clean.
