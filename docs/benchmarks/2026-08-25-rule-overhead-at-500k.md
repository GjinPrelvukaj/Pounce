# Rule overhead at 500k — over budget against a fixture, free against a website

> **Read the last section first.** The investigation below ends with the
> answer, and it changes what the headline number means: the rules cost a fixed
> ~11 µs per page of critical-path work, which is 13.2% of a crawl against a
> localhost fixture that answers instantly and **0.22%** of a crawl against
> anything with network latency in it.

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
- it is **not** CPU contention: the rules arm burns as much extra *CPU* as it
  does wall time, so the work is real (below);
- and it is **not** the instrument's shape: matching the crawl's issue density,
  batch size, link volume and detail strings moves the store-side figure by
  0.02 s (below).

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

### The instrument, brought all the way to the crawl's shape

The store-side seeder differed from a crawl in three more ways, and each was
closed and re-measured at 100k:

| Instrument | Pages only | With findings | Findings cost |
| --- | ---: | ---: | ---: |
| batch 5,000, no links, no detail | 1.335 s | 1.532 s | +0.196 s |
| batch 500, no links, no detail | 1.637 s | 1.869 s | +0.232 s |
| batch 500, **28 links/page** | 4.365 s | 4.551 s | +0.186 s |
| batch 500, 28 links/page, **detail strings** | 4.365 s | 4.579 s | +0.214 s |

Links quadruple both arms and leave the delta alone — so the `has_issue` update
is *not* paying for a cache that link inserts evicted, which was the leading
hypothesis and is now dead. Detail strings cost 28 ms across 200,000 findings.

**With the instrument matching the crawl in density, batch size, link volume and
detail bytes, the store-side cost of the rules' output at 100k is 0.214 s of
writing plus 0.200 s of indexing.** With 0.148 s of site rules and the
microbenchmark's 0.034 s of page-rule evaluation, that is **0.60 s against a
measured 1.53 s of wall and 1.6 s of CPU.**

### Every part is measured, and the parts do not add up

The obvious next move was to suspect the 341.6 ns/page page-rule figure of
having been taken on an easier page than the fixture serves. It was not:
`pounce-bench/benches/audit.rs` builds its records by rendering the bench
fixture's own pages and parsing them, links and all, with a quarter of the
corpus deliberately made mixed-content so that rule walks every link rather
than short-circuiting. That number stands.

So the account at 100k, with every component measured on the fixture's own
shape, is:

| Component | Measured |
| --- | ---: |
| Writing findings and flagging their pages | 0.214 s |
| Indexing the findings at the end of the crawl | 0.200 s |
| Site-rule pass | 0.148 s |
| Page-rule evaluation | 0.034 s |
| **Sum of the parts** | **0.60 s** |
| **Measured whole** | **1.53 s wall / 1.6 s CPU** |

**The parts account for 40% of the whole.** Every component has now been priced
against a fixture matching the crawl in issue density, link volume, batch size
and detail bytes; none of them is wrong on its own; and something in combining
them costs 0.9 s per 100,000 pages that none of them sees.

The caution stands even though the answer arrived: **a sum of component
benchmarks is not a system measurement.** Five separate instruments here each
said "this part is cheap", and the assembled system was two and a half times
their sum, because none of them contained the pipeline the parts run inside.
The end-to-end A/B is the only figure that was ever trustworthy for this
question.

### The answer: it depends entirely on what the bottleneck is

The hypothesis was that the batched writer is the pipeline's one funnel, so work
added inside it costs wall time roughly 1:1 while a component benchmark, having
no pipeline to stall, only ever sees the smaller number. That is testable
without touching a line of the crawl: **slow the fetch stage down and see
whether the cost survives.**

The A/B at 20k pages, twice each, with and without a 5 ms per-request delay:

| Fixture answers | Empty registry | Full ruleset | Difference | Share |
| --- | ---: | ---: | ---: | ---: |
| instantly (writer-bound) | 3.164 s, 3.183 s | 3.367 s, 3.414 s | **+0.22 s** | **6.9%** |
| after 5 ms (fetch-bound) | 100.640 s, 100.636 s | 100.861 s, 100.868 s | **+0.22 s** | **0.22%** |

**The absolute cost is identical — 0.22 s either way, to two decimal places.**
Only the denominator moved. The rules do a fixed ~11 µs of work per page, and
whether that is 6.9% or 0.22% of a crawl depends entirely on whether anything
else is waiting.

Which settles every loose end in this file at once:

- the parts *do* add up — ~11 µs/page against the ~6 µs/page the store-side
  components account for, the rest being the sink closure's own serialisation
  (`run_page`, building the row slice, the `has_issue` update) which no
  store-only benchmark contains;
- the share grows with corpus size because a bigger crawl spends more of itself
  writer-bound, not because the rules get more expensive;
- the CPU and wall costs matched because the work is real — it simply hides
  behind network latency when there is any.

**And it reframes Gate M2.** The 10% budget is being measured against the most
hostile denominator that exists: a localhost fixture with no latency, where the
crawler is bound by its own writer. That is the right benchmark for *throughput*
— it is why the fixture exists — and the wrong one for "what do the rules cost a
user". Against a site with 5 ms of latency, thirty audit rules cost **two parts
in a thousand**.

The gate should be re-judged on that basis rather than by making anything
cheaper. If a number is wanted for the budget, it should be stated the way this
table states it: a fixed per-page cost, and the two shares it produces at the
extremes.

### A first profile, and what it points at

macOS ships `sample`, so the profile did not have to wait. Both arms were run
alone at 200k and sampled for 15 s in the middle of the crawl.

The two profiles are nearly identical everywhere the work is: `sqlite3VdbeExec`
100 vs 105 samples, `fsync` 1,693 vs 1,767, `pwrite` 1,045 vs 1,063, the parser's
`extract` 592 vs 572. **No `pounce_audit` symbol appears in either profile's
top-of-stack list at all** — consistent with 341 ns/page, and one more reason to
believe that number.

The one large difference is waiting: `__psynch_cvwait` is **111,981 samples in
the rules arm against 83,050 in the plain one**, +35%. Blocked threads, not busy
ones.

**The hypothesis that fits every measurement in this file: the writer is the
pipeline's critical path, and work added there is not amortised.** The fetch and
parse stages are parallel and bounded; the batched writer is one stage that
everything funnels through. Work done inside it costs close to 1:1 in wall time
*and* stalls the stages behind it, which is exactly what "component costs 0.4 s,
system costs 1.5 s" looks like from the outside. A store-side benchmark has no
pipeline to stall, so it can only ever see the smaller number.

This is a hypothesis with sampling evidence, not a proven cause — the two 15 s
windows are at different points of their respective crawls, and `sample`'s
counts are thread-samples rather than time. Confirming it means instrumenting
the writer stage's occupancy directly, with and without the registry. If it
holds, the fix is not to make the rules cheaper but to move issue writing off
the critical path — a batch of its own, or a second connection — and that is a
design change with the "two writers on one file" hazard sitting next to it.

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
