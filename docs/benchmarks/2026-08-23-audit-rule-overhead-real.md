# Audit rule overhead, the real thirty — and a defect the measurement found

**Date:** 2026-08-23
**Host:** Apple M5, macOS 27.0, rustc 1.97.1, `--release`. Dev laptop, not a controlled rig.
**Supersedes:** [`2026-08-21-audit-rule-overhead.md`](2026-08-21-audit-rule-overhead.md),
which measured 30 **stand-in** rules, excluded all site rules, and had no
end-to-end run. Its headline — 116.5 ns/page, 0.044%, ×230 headroom — should
not be quoted again.

---

## 0. The finding that matters most

Measuring the site rules for the first time exposed a **defect that would have
made a large crawl never finish.**

Three site rules join on `links.target_url`: `links.orphan-page`,
`links.broken-internal`, and `indexability.blocked-but-linked`. The index for
that column, `links_target`, is deliberately deferred by migration 007 and
built at the end of the crawl — but `build_query_indices()` ran *after* the
site rules, so all three executed with no index available.

`links.orphan-page` correlates a subquery over `links` per candidate page, so
without the index SQLite re-scans the whole link table once per row:

```
plan: ["SEARCH p USING INDEX pages_depth (depth>?)",
       "CORRELATED SCALAR SUBQUERY 1",
       "SCAN l"]                                     <-- full table scan, per page
```

Measured at 10,000 pages (280k link rows): **45.0 s for that one rule**, and
`run_site` as a whole 45.03 s. The shape is O(pages x links), so 500k pages
against 14M edges would not have completed at all.

**Fix:** `Store::build_link_index()`, split out of `build_query_indices` and
called before the site rules that read it. This does not weaken migration
007's rule — that rule is that an index nothing reads *during* a crawl is not
**maintained** during one, which is what took throughput from 3,690 URL/s to
359. It is still one sorted bulk build; it now happens immediately before its
first reader instead of just after.

| `links.orphan-page`, 10k pages | Before | After |
|---|---:|---:|
| the rule alone | 45.0 s | 3.99 ms |
| `run_site`, all 11 rules | 45.03 s | 12.05 ms |

**~11,000x.** The query plan now reads
`SEARCH l USING COVERING INDEX links_target (target_url=?)`, and the harness
asserts that plan rather than printing it, so a future reordering fails a test
instead of quietly costing 45 seconds.

## 1. Page rules — the real nineteen

`cargo bench -p pounce-bench --bench audit`, 5,000 fixture pages parsed before
the timed section. 19 of the 30 rules are page rules; the other 11 are site
rules, measured in §2.

| Case | 5,000 pages | Per page |
|---|---:|---:|
| Empty registry (control) | 6.33 µs | 1.3 ns |
| All 19 shipped page rules | 1.714 ms | 342.8 ns |
| **Net cost of the rules** | **1.708 ms** | **341.6 ns** |

**2.9x the old stand-in figure** (116.5 ns/page), for **19 rules rather than
30**. Per rule the real ones cost about 4.6x what the stand-ins did.

### Per rule, so an expensive one names itself

| Rule | ns/page | | Rule | ns/page |
|---|---:|---|---|---:|
| `response.mixed-content` | 141.44 | | `description.too-long` | 6.59 |
| `title.too-short` | 45.86 | | `title.too-long` | 3.66 |
| `description.too-short` | 42.69 | | `content.empty-h1` | 1.23 |
| `media.missing-alt` | 21.22 | | `description.missing` | 1.22 |
| `indexability.canonical-elsewhere` | 10.74 | | `indexability.noindex` | 1.19 |
| `description.truncated-entity` | 7.21 | | the remaining 8 | ≤ 1.06 each |

Sum of the parts: 290.4 ns. The 51 ns gap to the 341.6 ns measured together is
the registry loop and the shared `Vec` growth.

**`response.mixed-content` is 41% of the whole page-rule budget**, because it
is the only rule that walks every link on the page — ~28 of them. To measure
that honestly the bench marks a quarter of the corpus https while its links
stay http, so the rule runs its full per-link walk instead of returning at the
scheme check. On an all-http site it costs almost nothing; on a site
mid-migration to https it costs this.

### What actually makes a rule expensive: firing, not checking

`title.too-long` and `title.too-short` are the same code — same field, same
comparison, same `format!("{chars} characters")` on a hit. They cost **3.66**
and **45.86** ns/page. The only difference is that the fixture's titles run
~20 characters, so `too-short` fires on nearly every page and `too-long` on
none.

So a page rule's cost is dominated by how often it *fires* — the `String`
allocation and the push, ~42 ns per finding — not by the work of deciding.
Rule overhead is therefore proportional to how broken the site is. That is a
defensible property, but it means a per-page figure measured on a clean corpus
would understate the real cost, which is why this one is measured on a corpus
that triggers findings.

## 2. Site rules — the eleven, measured for the first time

`cargo test --release -p pounce-audit --test site_rule_overhead -- --ignored --nocapture`

Site rules run once against the finished database, so they cannot be expressed
per page. Stores are seeded with the fixture's shape — 28 links per page,
unique titles and bodies, ~12% missing descriptions, mostly 200s — plus one
deterministic finding for **every** rule. Those findings are asserted after the
timing: a query that returned instantly because it matched nothing would have
timed an empty branch, not a rule.

| Rule | 10k | 100k | 500k |
|---|---:|---:|---:|
| `description.duplicate` | 2.67 ms | 34.36 ms | 325.92 ms |
| `links.orphan-page` | 3.99 ms | 45.74 ms | 257.12 ms |
| `content.duplicate-body` | 2.23 ms | 31.17 ms | 207.10 ms |
| `indexability.canonical-non-200` | 0.79 ms | 10.07 ms | 59.22 ms |
| `indexability.canonical-chain` | 0.79 ms | 10.26 ms | 58.70 ms |
| `links.broken-internal` | 0.73 ms | 8.97 ms | 52.64 ms |
| `title.duplicate` | 0.56 ms | 5.62 ms | 31.02 ms |
| `indexability.blocked-but-linked` | 43 µs | 70 µs | 96 µs |
| `response.redirect-loop` | 11 µs | 13 µs | 18 µs |
| `media.broken-image` | 17 µs | 22 µs | 26 µs |
| `media.oversized-image` | 7 µs | 8 µs | 10 µs |
| **`run_site` total** | **12.05 ms** | **147.75 ms** | **848.52 ms** |

Scaling is close to linear: 12.3x from 10k to 100k, 5.7x from 100k to 500k.
Nothing here is quadratic — which is precisely what was *not* true before §0's
fix, and is why this table is the one that had to be produced before the gate
could be judged.

Against the published 500k crawl wall time of 133.9 s, the whole site-rule pass
is **0.63%**.

`title.duplicate` is 10x cheaper than `description.duplicate` at every size,
because `pages_title` is a crawl-time index and there is no equivalent on
`meta_description`. Worth an index if the figure ever matters; at 326 ms in a
134-second crawl it does not yet.

## 3. End to end — the number the gate actually asks about

`cargo test --release -p pounce-seo --lib rules_on_versus -- --ignored --nocapture`
(`RULE_AB_PAGES` sets the corpus.)

The same `crawl()` run twice: once with `register_all`, once with an empty
registry, everything else identical. Interleaved pairs with the leading arm
alternating, because unpaired blocks drift. This includes **writing the issues
the rules find**, which is work that exists only because the rules ran and
therefore belongs in the number.

| | 10,001 pages | 100,001 pages |
|---|---:|---:|
| Empty registry, median of 5 | 1.3269 s | 18.5761 s |
| Full ruleset, median of 5 | 1.3965 s | 19.5575 s |
| Difference | +69.7 ms | +981 ms |
| **Share of crawl wall time** | **5.25%** | **5.28%** |
| Issues written | 22,431 | 223,764 |

> **One claim below was overturned on 2026-08-25.** The share is *not* flat once
> the corpus reaches 500k — it is 13.2% there — and the share itself turns out to
> be a property of the fixture rather than of the rules: the same work is 0.22%
> of a crawl with network latency in it. See
> [`2026-08-25-rule-overhead-at-500k.md`](2026-08-25-rule-overhead-at-500k.md).
> Everything else in this file stands, including the per-rule tables, which that
> measurement leans on.

**The share is flat across a 10x change in corpus size** — 5.25% to 5.28% —
which is what makes this an answer rather than an extrapolation. Issue count
scales linearly too (22.4k → 223.8k), so the per-page work the rules cause is
constant.

One pair at 100k is an outlier in both arms (24.95 s / 20.60 s against ~19.5 /
~18.5); medians rather than means are reported for that reason.

### Why 5.3% and not the 0.13% the microbenchmarks imply

341.6 ns/page of page-rule evaluation over 100k pages is 34 ms, and the site
rules add 148 ms — together 182 ms, against a measured 981 ms. **The missing
800 ms is writing 223,764 issue rows.** Evaluating the rules is nearly free;
recording what they find is not, and it is the larger half by 4x.

That also means the figure is sensitive to how broken the site is. This fixture
produces ~2.2 issues per page. A clean site would cost less; a worse one more.

### This is a conservative reading

The A/B crawls 10k pages in ~1.33 s, roughly 7,500 pages/s, because the fixture
server shares the process. A real crawl over the network is slower per page —
the published figure through the binary is 3,690 URL/s at 10k — so the same
fixed rule cost is a *smaller* fraction of it. 5.3% is the share when the crawl
is as fast as it can possibly be.

## 4. The verdict

**Gate M2's third item passes.** Rule execution adds **5.3%** of crawl wall
time against a 10% budget — about **1.9x headroom**, measured at two sizes with
the share flat between them.

That headroom is far narrower than the ×230 the superseded document claimed,
and the honest reading is that the old figure was measuring the wrong thing:
stand-in rules, no site rules, and no issue writing at all.

## 5. What this does **not** establish

- **No end-to-end A/B at 500k.** 10k and 100k agree to within 0.03 percentage
  points, so the trend is flat, but the largest published crawl size has not
  been run both ways. The site-rule table does cover 500k.
- **The per-page figure depends on issue density.** ~2.2 issues per page here.
  The 5.3% would move on a corpus that is cleaner or worse, and §3 explains
  which way.
- **`response.mixed-content` was measured on a deliberately hostile corpus** —
  a quarter of pages https with http links. That is the shape of a site
  mid-migration, not of every site; on all-http content the rule short-circuits
  and costs nothing.
- **Peak memory during the rule pass was not measured.** The 400 MB gate is
  M1's and was measured without rules running.
