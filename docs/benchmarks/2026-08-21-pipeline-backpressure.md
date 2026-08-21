# Pipeline backpressure — 500k URLs

**Date:** 2026-08-21
**Host:** Apple M5, macOS 27.0, rustc 1.97.1, `--release`
**Command:**

```bash
cargo test --release -p pounce-bench --test pipeline_rss -- --ignored --nocapture
```

The probe sends generated, normalised `CrawlUrl`s through the T1.17 pipeline.
Its fetch stage allocates a 4 KiB body for every URL and its writer is slowed
deliberately. With no backpressure, 500k bodies could retain about 2 GiB. Each
of the three handoffs instead has capacity 32.

| Workload | Peak RSS | Wall time |
|---:|---:|---:|
| 50,000 URLs | 3.52 MiB | included in 5.35s combined test |
| 500,000 URLs | 3.38 MiB | included in 5.35s combined test |

Measured RSS growth was **0.00 MiB** (the 500k sample was 0.14 MiB lower).
The test permits at most 8 MiB of growth to absorb allocator and sampler noise.

## What this proves

Pipeline retention is bounded by channel capacity and worker concurrency, not
by total URL count. The same child-process sampler used for competitor runs
measures both workloads, so internal allocator counters are not trusted.

## Caveat

This isolates pipeline memory with a crawl-shaped response payload. It does not
include network I/O, HTML extraction, SQLite indices, or the future CLI. Gate
M1 still requires the full 500k fixture crawl and the separate `< 400 MB` peak
RSS target; these numbers must not be presented as that end-to-end result.
