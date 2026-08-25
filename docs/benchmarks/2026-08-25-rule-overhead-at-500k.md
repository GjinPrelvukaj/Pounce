# Rule overhead at 500k — the share is not flat, and it is over budget

**Date:** 2026-08-25
**Host:** Apple M5, macOS 27.0, `--release`. A laptop that had been compiling
and crawling for hours; see *Is this the machine?* below.
**Supersedes one claim in**
[`2026-08-23-audit-rule-overhead-real.md`](2026-08-23-audit-rule-overhead-real.md):
that the rules' share of crawl wall time is flat with corpus size. Everything
else in that document stands.

---

## The number

The same end-to-end A/B, at the size that file listed as a known gap. Two pairs,
alternating order, 500,001 pages each run:

| Arm | Run 1 | Run 2 |
| --- | ---: | ---: |
| Empty registry | 152.14 s | 151.56 s |
| Full ruleset | 174.85 s | 175.01 s |

**+23.1 s, or 13.2% of the crawl.** Gate M2's budget is 10%.

The runs are tight — 0.6 s of spread inside each arm — so this is not noise.
1,112,870 issues were written by the rules arm.

| Corpus | Rules' share |
| --- | ---: |
| 10k (2026-08-23, idle machine) | 5.25% |
| 100k (2026-08-23, idle machine) | 5.28% |
| 100k (2026-08-25, loaded machine) | 7.61% |
| **500k (2026-08-25, loaded machine)** | **13.2%** |

The 2026-08-23 write-up said the flat share between 10k and 100k "is what makes
this an answer rather than an extrapolation". Extended to 500k, the share is not
flat; it roughly doubles.

## Is this the machine?

Partly, and not enough. The same laptop measured
[8% slower than 2026-08-23](2026-08-25-no-regression-recheck.md) on an identical
test, and a *proportional* slowdown leaves a ratio unchanged — both arms carry
it. At 100k the share moved 5.28% → 7.61% under load, so environment is worth
about 2 percentage points here. That does not account for 13.2%.

## What accounts for it, and what does not

Every store-side instrument was run at 500k to price the parts:

| Component | Measured at 500k | Scaled to the crawl's 1.11M issues |
| --- | ---: | ---: |
| Page-rule evaluation | 341.6 ns/page | ~0.17 s |
| Site-rule pass (`run_site`) | 848 ms | ~0.85 s |
| Writing findings + `has_issue` on the write path | +4.61% of 10.5 s = 0.48 s (333k issues) | ~1.6 s |
| Indexing the findings in `build_query_indices` | +491 ms (333k issues) | ~1.6 s |
| **Total explained** | | **~4.2 s** |

Against a measured **23.1 s**. **About four fifths of the cost is not yet
localised**, and every obvious candidate has now been priced and ruled out:

- it is **not** the site rules (0.85 s, and their scaling is linear — that was
  fixed on 2026-08-23);
- it is **not** the deferred index build over the issue rows (0.49 s at 333k,
  and the shape is linear);
- it is **not** the `has_issue` update on the write path, which *shrinks* as a
  share with scale — 8.94% of the write path at 100k, 4.61% at 500k, because
  page writing itself slows with the file while the per-issue cost does not;
- it is **not** issue density, and **not** the writer's batch size — both were
  suspected of making the instrument lie, and both were tested (below);
- and it is **not** CPU contention: the rules arm burns as much extra *CPU* as
  it does wall time, so the work is real (below).

### Two more candidates, tested and ruled out

The store-side arms wrote **0.67 issues per page** where the crawl writes
**2.24**, and they batched **5,000 rows** where the crawl's `Writer` batches
**500** — ten times the commits. Both were suspected of making the instrument
under-price the real write path, so the seeder now takes a density and a batch
size, and both were varied at 100k:

| Density | Batch | Pages only | With findings | Findings cost |
| ---: | ---: | ---: | ---: | ---: |
| 0.67 | 5,000 | 1.321 s | 1.440 s | +0.118 s |
| 2.0 | 5,000 | 1.335 s | 1.532 s | +0.196 s |
| 2.0 | 500 | 1.637 s | 1.869 s | +0.232 s |

Tripling the density raises the cost of findings by 1.7x, not 3x — **sublinear**,
so scaling the earlier figures by density was pessimistic rather than optimistic.
Matching the crawl's batch size raises *both* arms by 0.3 s and moves the delta
by 0.036 s: the commit overhead is real and it is not about findings.

At the crawl's own density and batch size, then, the store-side cost of the
rules' output at 100k is 0.232 s of writing plus 0.211 s of indexing. With
0.148 s of site rules and 0.034 s of page-rule evaluation that is **0.63 s
against a measured 1.67 s** — the same two-thirds unaccounted for as at 500k.
Whatever this is, it is a constant *fraction*, not a scale effect.

### The CPU answer: the work is real

Each arm was then run alone, in its own process, under `/usr/bin/time -l` at
100k, twice each:

| Arm | Wall | User | Sys | User + sys |
| --- | ---: | ---: | ---: | ---: |
| Rules | 21.78 s, 21.27 s | 20.17 s, 20.22 s | 12.05 s, 11.80 s | **32.1 s** |
| Empty registry | 19.98 s, 19.90 s | 19.45 s, 19.46 s | 11.11 s, 11.03 s | **30.5 s** |

**+1.53 s of wall and +1.6 s of CPU.** They match, which settles it: the rules
are doing real work, not losing a race for a saturated core. Contention is ruled
out, and so is any fix that consists of scheduling them differently.

Note the split: **+0.74 s user and +0.85 s system.** More than half of the cost
is in the kernel, which is where writing goes.

### The hypothesis left standing

**The store-side fixture writes no links, and the crawl writes twenty-eight per
page.** `common::record(i)` has an empty `links` vector, so in the instrument a
transaction contains pages, `page_detail` rows and issues — and in a crawl the
same transaction also carries ~14,000 link inserts. The `has_issue` update lands
on a page row whose B-tree pages have had all of that churned through them since
they were written, and half the missing time being *system* time is what that
would look like.

**The next experiment gives the seeded records links** and re-runs the write-path
arm. If the delta grows to close the gap, the cost is a cache effect around
`has_issue` and the fix is to set the flag in the insert rather than as a second
statement — which is a schema-level change, not a rule-level one, and belongs in
a task of its own.

## What to do about it

Nothing yet, and deliberately. This is a measurement, not a fix, and the fix
depends on a cause that is not established. Two things follow immediately:

1. **Gate M2's tick now carries a caveat rather than a clean pass.** The budget
   holds at 10k and 100k and is exceeded at 500k. `PLAN.md` and `CLAUDE.md` say
   so beside the 5.3%.
2. **The rules' share moves with how broken the site is.** The 2026-08-23 file
   already said this — the figure includes writing what the rules find, at ~2.2
   issues per page on this fixture — and it matters more than it looked. A
   cleaner site pays less; a worse one pays more than 13.2%.

## Reproduce

```bash
RULE_AB_PAGES=500000 RULE_AB_PAIRS=2 \
  cargo test --release -p pounce-run --lib rules_on_versus -- --ignored --nocapture

FLAG_COST_PAGES=500000 FLAG_COST_PAIRS=3 \
  cargo test --release -p pounce-store --test issue_flag_write_cost -- --ignored --nocapture
```

`RULE_AB_PAIRS` exists because five pairs at 500k is ten crawls of ~5.8 GB in
one temporary directory; the outputs are now deleted as they are measured.
