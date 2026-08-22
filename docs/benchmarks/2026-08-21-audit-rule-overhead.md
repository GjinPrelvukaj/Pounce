# Audit rule overhead — the Gate M2 10% budget

> **SUPERSEDED 2026-08-23.** Every figure below was measured against 30
> **stand-in** rules, excluded all 11 site rules, and had no end-to-end run.
> The real thirty cost **2.9x more per page** for **19** page rules, and the
> site rules — unmeasured here — turned out to hide a defect that made
> `links.orphan-page` take 45 s on a 10k store. Do not quote the 116.5 ns/page
> or ×230 headroom figures. See
> [`2026-08-23-audit-rule-overhead-real.md`](2026-08-23-audit-rule-overhead-real.md).
> Kept for the record of what was known on 2026-08-21.


**Date:** 2026-08-21
**Host:** Apple M5, macOS 27.0, rustc 1.97.1, `--release`. Dev laptop, not a controlled rig.
**Command:** `cargo bench -p pounce-bench --bench audit`
**Fixture:** 5,000 pages, `SiteGraph::generate(seed 42)`, rendered and run
through `parse_body` **before** the timed section, so this measures the rule
pass and nothing else.

---

## 1. Result

| Case | 5,000 pages |
|---|---:|
| Empty registry (control) | 6.56 µs |
| 30 page rules | 589.19 µs |

Subtracting the loop: **116.5 ns per page for 30 rules — 3.9 ns per rule
evaluation.**

## 2. Against the gate

Gate M2: *rule execution adds under 10% to crawl wall time.*

| Crawl | Rule time | Crawl wall | Share | Budget | Headroom |
|---|---:|---:|---:|---:|---:|
| 100k | 12 ms | 18.8 s | **0.062%** | 1.9 s | ×161 |
| 500k | 58 ms | 133.9 s | **0.044%** | 13.4 s | ×230 |

**The budget is not close to binding.** Rules would have to be roughly **230×
more expensive** than these before they consumed their allowance at 500k.

That is the expected shape rather than a surprise: a page rule reads fields off
a struct already in cache, while the crawl around it is doing network I/O and
SQLite writes. The design choice that produces it is `PageRule` being handed a
`&PageRecord` and nothing else — a rule that could issue a query would not be
3.9 ns, and the type system is what prevents one from trying.

## 3. What this does **not** establish

1. **These are not the real thirty rules.** They are stand-ins with the shape of
   one — read a field, compare, sometimes push — with thresholds alternating so
   about half fire. A rule that compiles a regex, walks every link, or hashes
   something is not represented. **The honest figure arrives when the batches
   land**, and this number should be re-taken then.
2. **Site rules are excluded entirely.** They are `GROUP BY`/join queries over
   the finished database, and their cost scales with row count rather than with
   pages. Thirteen of the thirty are site rules, so **a large part of the real
   answer is not measured here at all.**
3. **The crawl wall times it divides into are single runs** from
   [`2026-08-21-scaling-fix.md`](2026-08-21-scaling-fix.md). At ×230 headroom
   the conclusion survives any plausible error in them, but the percentages
   should not be quoted to three digits.
4. **No end-to-end confirmation.** `pounce crawl` has no flag to enable rules,
   so a crawl-with-rules against crawl-without has never been run. That belongs
   with the CLI surface (T6.1) and the real rules.

## 4. Still open from T2.3

Adding the `issues` table and the two `pages` columns showed three-run medians
of **18.8 s → 20.0 s at 100k**, with ranges 18.5–19.3 and 18.4–21.7. The ranges
overlap and no mechanism explains a 10% cost, so it is recorded as neither
established nor dismissed. **It is unrelated to rule execution** — it is the
storage change, not the audit pass — but it lands in the same gate's budget and
needs a controlled re-measure before Gate M2 is claimed.
