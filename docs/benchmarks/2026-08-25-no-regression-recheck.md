# Re-baselining the crawl, and a regression that was not one

**Date:** 2026-08-25
**Host:** Apple M5, macOS 27.0, `--release`. **The same laptop that had been
compiling continuously for five hours** — which turns out to be the finding.

---

## Why this was run

`docs/benchmarks/` last measured a full crawl on 2026-08-21, before M2's thirty
audit rules, migration 012's row split and migration 013's `has_issue` column.
"A performance regression is a broken build" only means something if someone
re-measures, so the crawl was re-run at 10k and 100k.

## What came back

`bench-runner`, three runs each, current `HEAD`:

| Fixture | Wall | Peak RSS | URLs | 2026-08-21 |
| --- | ---: | ---: | ---: | --- |
| 10k | 1.6 s, 1.6 s, 1.6 s | 25 MB | 10,001 | 1.6 s / 25 MB |
| 100k | 21.9 s, 22.5 s, 21.8 s | 70–71 MB | 100,001 | 17.8 s / 75 MB |

10k is unchanged to the tenth of a second. **100k looks 23% slower.**

## Chasing it

The 100k figure from 2026-08-21 predates the audit rules, so part of the gap is
expected: 2026-08-23 measured the full ruleset at 5.3% of crawl wall time. That
accounts for about one second of four.

The end-to-end A/B that produced the 5.3% was re-run on `HEAD` — the same
`crawl()` twice, once with `register_all` and once with an empty registry,
interleaved, medians of five:

| | Empty registry | Full ruleset | Difference | Share |
| --- | ---: | ---: | ---: | ---: |
| 2026-08-23 | 18.576 s | 19.558 s | +981 ms | 5.28% |
| **HEAD, today** | **20.226 s** | **21.892 s** | **+1,666 ms** | **7.61%** |

**Both arms moved**, including the one that runs no rules at all. Whatever this
is, it is not the rules.

The suspect was T4.4, which moved the runner onto `run_controlled_pipeline` —
one shared lifecycle, an admission check and a pause gate per URL. So the tree
was checked out at `7ff4654` (migration 013, the commit immediately before
T4.4) into a worktree and the identical test run again, on the same machine, in
the same session:

| | Empty registry | Full ruleset | Difference | Share |
| --- | ---: | ---: | ---: | ---: |
| `7ff4654`, today | 20.239 s | 21.561 s | +1,322 ms | 6.13% |
| `HEAD`, today | 20.226 s | 21.892 s | +1,666 ms | 7.61% |

**The pre-T4.4 tree is the same speed as `HEAD`** — 20.24 s against 20.23 s on
the arm that does no auditing, which is as close as this measurement resolves.
The 100k crawl has not regressed. The machine has: five hours of continuous
release builds, a hot laptop, and a session's worth of background processes cost
roughly 8% against 2026-08-23's numbers, and that is the entire gap.

## What to take from it

1. **No regression.** Nothing to fix, and the two hours it would have taken to
   "optimise" the controlled pipeline would have been spent on noise.
2. **Rule overhead is environment-sensitive.** 5.28% on an idle machine,
   6.13–7.61% on a loaded one, against Gate M2's 10% budget. The gate still
   passes, but with less headroom than the published number implies, and a
   future rule batch should be measured on a quiet machine before anyone
   concludes it is free.
3. **Cross-day comparisons on a working laptop are worth about ±10%.** The way
   to tell a code change from machine drift is to build the old commit and
   measure it beside the new one *in the same session* — which costs one
   worktree and four minutes, and is the only reason this file says "no
   regression" rather than "23% slower, cause unknown".

## Reproduce

```bash
cargo build --release -p pounce-seo -p pounce-bench
./target/release/bench-runner --pages 100000 --seed 42 \
  --tool 'pounce=/path/to/wrapper.sh {url}' --count 'pounce=/path/to/count.sh'

RULE_AB_PAGES=100000 cargo test --release -p pounce-run --lib rules_on_versus -- --ignored --nocapture
```

The wrapper removes the output file first — `pounce crawl` refuses to overwrite
one — and the count script reads `select count(*) from pages` out of it.
