# Pounce re-taken — 10k and 100k, with the whole product running

**Date:** 2026-08-29
**Host:** Apple M5, macOS 27.0, rustc 1.97.1, `--release`. Dev laptop, not a controlled rig.
**Build:** Pounce `0.0.1` at `f9a4921` — engine, 30 audit rules, read-path index
build, and the sitemap comparison, all on by default.
**Fixture:** `fixture-site --pages {10000,100000} --seed 42`, localhost.
**Command:** `pounce crawl http://127.0.0.1:8099/ --output run.pounce --quiet`,
timed with `/usr/bin/time -l`.

**Why this exists.** The published head-to-head carries a warning that Pounce's
half is superseded, and it has carried it for eight days. Everything about this
project's positioning rests on a number nobody had re-measured since the engine
milestone. This re-takes our half. **It is not a head-to-head** — see §4.

---

## 1. Result

Medians. Three runs at 10k, five at 100k.

| Fixture | Wall | URLs/s | Peak RSS | Pages | Distinct | Findings |
|---|---:|---:|---:|---:|---:|---:|
| 10k | **2.02 s** | 4,951 | **28 MB** | 10,001 | 10,001 | 22,431 |
| 100k | **24.35 s** | 4,107 | **81 MB** | 100,001 | 100,001 | 223,764 |

Spread: 10k 1.85–2.06 s, 27–30 MB. 100k 22.41–25.19 s, 78–84 MB.

Integrity verified on every run: pages equals distinct URLs, so no duplicates
and no losses at either scale.

## 2. This is a slower number than the last one, and that is the point

The last figures for this fixture were **1.9 s at 10k and 19.3 s at 100k**
([`2026-08-21-scaling-fix.md`](2026-08-21-scaling-fix.md)). Today is 2.02 s and
24.35 s — 6% and 26% slower.

**Those figures measured a crawler that did not audit anything.** They were
taken during M1, before the audit engine existed. Today's run additionally:

- executes **30 audit rules on every page**, producing 223,764 findings at 100k
  — measured separately at ~11 µs/page
  ([`2026-08-25-rule-overhead-at-500k.md`](2026-08-25-rule-overhead-at-500k.md)),
  so ~1.1 s of the 100k figure;
- builds the read-path indices the grid needs, including the `has_issue`
  denormalisation (migration 013) and the declared composites (M3);
- fetches, parses and stores **the site's sitemap** — 50,000 URLs at 100k.

So the honest statement is not "6–26% slower". It is: **the product now does
three jobs the benchmarked crawler did not do, for 26% more wall time at 100k.**
The remaining share after the rules is the index build and the sitemap pass; the
two are not separated here, and that is a gap in this measurement rather than a
claim.

## 3. Scaling

10× the pages costs **12.05× the wall time** (2.02 s → 24.35 s), against
9.76× for the same step measured before the audit engine existed. Memory grows
2.9× for 10× the pages.

The super-linear share is not the crawl loop; it is the end-of-crawl work, which
grows with the number of rows rather than with the number of requests. Nothing
here re-measures 500k, and the M1 gate's 500k figure stands as previously
published.

## 4. What this does not establish

1. **It is not a head-to-head.** FreeCrawl is not installed on this machine and
   was not re-run. The competitor half of
   [`2026-08-21-freecrawl-head-to-head-10k.md`](2026-08-21-freecrawl-head-to-head-10k.md)
   — 131.82 s and 720 MB at 10k, FreeCrawl 0.9.6 at its best configuration —
   still stands as measured on that date, but **a table pairing today's Pounce
   with last week's FreeCrawl is not a benchmark and is not published here.**
   T5.7 stays open until both halves are taken on the same day.
2. **Localhost only.** No network variance. On a real site both tools are bounded
   by the target server and the gap narrows to the point of being invisible to a
   user. This remains the project's own Risk #1.
3. **Screaming Frog is still untested**, and it is the tool users actually switch
   from.
4. **One machine, one OS.**

## 5. 500k was attempted and thrown out

The same re-take was run at 500k and is **not published**. Three runs of
identical work gave **189.08 s, 217.77 s and 1,441.57 s** — a 6.6× spread, which
is not measurement noise.

The cause was found rather than guessed at: `vm.swapusage` showed **12.1 GB of
13.3 GB in use**, with 176 MB of physical memory free. A 500,000-page crawl
writing a 5.8 GB database on a thrashing laptop is measuring the swap file. The
run was abandoned and the 2026-08-25 figure (175.3 s, 234 MB) stands with its
date, on a build that predates the sitemap pass.

Recorded because it is the rule: a benchmark that quietly drops its bad runs is
an advertisement, and one taken on a swapping machine is not a benchmark at all.
Page integrity was intact in all three — 500,001 pages, 500,001 distinct.

## 6. A finding this run produced

At 100k the fixture's sitemap lists ~97,000 URLs and `sitemap_urls` holds
exactly **50,000** — `MAX_LOCATIONS`, hit silently. The tree says "N more not
listed" when it bounds a folder and the workbook says what did not fit; this
path says nothing, so a crawl of a large site reports a sitemap smaller than the
one the site published. Recorded as a task rather than fixed here, because a
benchmark run is not the place to change behaviour.

## Reproduce

```bash
cargo build --release -p pounce-seo -p pounce-bench
./target/release/fixture-site --pages 100000 --seed 42 --port 8099 &
/usr/bin/time -l ./target/release/pounce crawl http://127.0.0.1:8099/ \
  --output run.pounce --quiet
sqlite3 run.pounce "SELECT count(*), count(DISTINCT url) FROM pages"
```
