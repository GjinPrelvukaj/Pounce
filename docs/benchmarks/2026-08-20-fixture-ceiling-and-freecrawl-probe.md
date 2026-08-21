# Fixture ceiling and a preliminary FreeCrawl probe

**Date:** 2026-08-20
**Machine:** Windows 11, dev laptop. Not a controlled benchmark rig.
**Status:** Preliminary. **Not** the Gate M0 competitor baseline — see Caveats.

---

## 1. Fixture serving ceiling (the control measurement)

The most important number in the harness. If the fixture cannot serve faster
than the crawlers under test, every benchmark measures the fixture.

```
concurrency   1:      4832 req/s      47.8 MiB/s
concurrency  16:     17052 req/s     168.8 MiB/s
concurrency  64:     16934 req/s     167.6 MiB/s
concurrency 200:     16498 req/s     163.3 MiB/s
```

**~17,000 req/s, flat from concurrency 16 upward.** The fixture is not a
bottleneck for any crawler we expect to test, by two to three orders of
magnitude.

Reproduce:

```bash
cargo test --release -p pounce-bench --test fixture_throughput -- --ignored --nocapture
```

This ran because a FreeCrawl probe reported 1,193ms average response times
against localhost, and the fixture was the obvious suspect. It was not the
cause — worth recording, because "the fixture is fine" is the premise every
later benchmark rests on.

## 2. Parse throughput (Rust side, no network)

```
extract/lol_html (10KB pages)   186 MiB/s   ≈ 19,800 pages/sec
render_page                     2.09 µs
```

**Implication worth carrying into M1:** at ~19,800 pages/sec of pure parsing,
parsing will never be the bottleneck in a polite crawler. "Rust parses HTML
faster than Node" is true and largely irrelevant. The defensible advantage is
memory behaviour and staying responsive during a large crawl — benchmark
headlines should lead with peak RSS, not URLs/sec.

## 3. FreeCrawl probe — indicative only

FreeCrawl 0.9.5, built from source, Node CLI, HTTP-only (no JS rendering).

| Setting | Pages | Wall | URLs/s | Peak RSS | Failures | Exit |
|---|---:|---:|---:|---:|---:|---:|
| `--concurrency 200 --rps 100000` | 5,000 | 181.5s | 28 | 435 MB | 121 | 1 |

Peak RSS is process-tree-wide (parent plus all descendants).

The progress log shows long stalls rather than uniformly slow requests:

```
t=30s → t=42s     ~100 URLs   (12s stall)
t=62s → t=90s     ~130 URLs   (28s stall)
t=92s → t=125s    ~130 URLs   (33s stall)
```

A 33-second pause against a 435 MB heap is consistent with garbage collection
or a synchronous SQLite flush. Suggestive of the runtime tax the project's
positioning is built on — but not established.

### Caveats — read before quoting any of this

1. **`--concurrency 200` is 10× FreeCrawl's default of 20.** It appears to have
   saturated its own event loop; the 1,193ms average latency is internal
   queueing, not fixture slowness. A fair benchmark gives a competitor its
   *best* configuration. This was its worst.
2. **`exit 1` and 121 failed requests.** ~~A run that did not cleanly
   succeed.~~ **Corrected 2026-08-21:** FreeCrawl exits non-zero whenever any
   status ≥ 400 appears, which is correct CI behaviour for an SEO tool and which
   the fixture triggers on purpose. The exit code says nothing about the run.
   The 121 failed requests were real and remain a genuine problem.
3. **One configuration, one run, one machine.** No repetition, no variance.
4. **Localhost only.** Removes network variance, which is what makes it
   reproducible, but neither tool is under conditions any real crawl has.

**What survives the caveats:** the fixture serves 17,000 req/s and FreeCrawl
consumed 28. Even a 10× improvement from better tuning leaves it ~60× below
what the fixture can feed it. Configuration affects the number, not the
conclusion that the crawler — not the harness — is the limiting factor.

## 4. Decision

The Gate M0 competitor baseline was **deliberately skipped** on 2026-08-20. A
full 100k FreeCrawl run costs 30–60 minutes per configuration, and the
head-to-head that actually decides the project's premise belongs at the M1
gate, when Pounce can crawl and both sides can be measured together.

**The check is deferred, not dropped.** Gate M1 requires it.

## 5. Redo this properly at Gate M1

- Sweep FreeCrawl concurrency (20 / 50 / 100) and report its **best** result.
- Full 100k fixture, three runs per tool, report median and spread.
- Verify actual crawled counts from each tool's own output — do not assume a
  tool crawled everything it was asked to. `bench-runner` currently assumes
  this, and `freecrawl --json` emits a real count that should be read instead.
- Record machine spec, OS, and tool versions alongside the numbers.
- Publish failures and timeouts in the table rather than dropping them.
