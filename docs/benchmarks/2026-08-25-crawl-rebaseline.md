# The crawl, re-baselined on the shipping build

**Date:** 2026-08-25
**Host:** Apple M5, macOS 27.0, `--release`. A laptop that had been building and
crawling for hours and measures about
[8% slower than an idle one](2026-08-25-no-regression-recheck.md).
**Command:** `bench-runner --pages N --seed 42` against the deterministic
fixture, with a wrapper that removes the output first (`pounce crawl` refuses to
overwrite) and a counter that reads `select count(*) from pages` back out of it.

---

| Fixture | Pages | Wall | URLs/s | Peak RSS | Verified |
| --- | ---: | ---: | ---: | ---: | --- |
| 10k | 10,001 | 1.6 s (3 runs, identical) | 6,251 | 25 MB | 10,001 |
| 100k | 100,001 | 21.9 s (21.8, 22.5, 21.8) | 4,566 | 70–71 MB | 100,001 |
| 500k | 500,001 | 175.3 s | 2,852 | 234 MB | 500,001 |

**These are the first published figures taken with the thirty audit rules
running.** The previous set — 1.6 s / 17.8 s / 133.9 s, from 2026-08-21 —
predates M2 entirely and describes a crawler that audits nothing. They are not
comparable and the README no longer quotes them.

## What moved, and why

- **10k is unchanged to the tenth of a second**, which is the useful control:
  whatever else changed between the two dates, it did not cost anything at that
  size.
- **100k went 17.8 s → 21.9 s** and **500k went 133.9 s → 175.3 s.** Three
  things are in that: the audit rules (see below), the ~8% the machine has
  drifted, and migrations 012 and 013, which were each measured neutral or
  better on the write path when they landed.
- **Peak RSS went 279 MB → 234 MB at 500k.** Memory improved while wall time
  regressed, which is what migration 012's narrower `pages` row was for.

## The rules' share, stated properly

Thirty rules cost a fixed **~11 µs per page**. Against this fixture — localhost,
answering instantly, so the crawler is bound by its own writer — that is 13.2%
of the 500k crawl. Add 5 ms of per-request latency and the identical 0.22 s of
work becomes **0.22%** of the crawl. See
[`2026-08-25-rule-overhead-at-500k.md`](2026-08-25-rule-overhead-at-500k.md).

Both numbers are true and neither is "the" number. A benchmark against a
zero-latency fixture measures the crawler's ceiling, which is the point of it;
it also makes every fixed per-page cost look as large as it can possibly look.

## Reproduce

```bash
cargo build --release -p pounce-seo -p pounce-bench

cat > /tmp/pounce-wrap.sh <<'SH'
#!/bin/sh
OUT=/tmp/bench-run.pounce
rm -f "$OUT" "$OUT-wal" "$OUT-shm"
exec ./target/release/pounce crawl "$1" --output "$OUT" --quiet
SH
chmod +x /tmp/pounce-wrap.sh

./target/release/bench-runner --pages 100000 --seed 42 \
  --tool 'pounce=/tmp/pounce-wrap.sh {url}' --count 'pounce=/tmp/count.sh'
```

`/tmp/count.sh` prints `select count(*) from pages` from the output file; without
one the runner reports the crawled count as unknown rather than guessing.
