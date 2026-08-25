# The two queries the results screen lives on

**Date:** 2026-08-25
**Build:** `--release`, the test binary run directly.
**Stores:** a real 100k crawl of the bench fixture (100,001 pages, 2,899,792
links, 223,764 findings, 1.1 GB) and the 500k seeded store (500,000 pages,
333,333 findings, no links, 681 MB).

| Query | 100k crawl | 500k seeded |
| --- | ---: | ---: |
| `issue_overview` — the findings rail | **108 ms** (223,764 findings) | **101 ms** (333,333 findings) |
| `page_detail` — one row's pane | **6.8 ms** | 0.6 ms |

## Why these two

They are the only queries in the app that are not a windowed `query_rows`, and
each was adopted on an argument rather than a measurement:

- **`issue_overview` is a `GROUP BY` over every issue in the file**, and the
  results screen re-runs it once a second while a crawl is writing. T4.21 held
  one in flight at a time on the theory that at scale it could take longer than
  the second between ticks. At **~100 ms it does not**, on either store — the
  in-flight guard is cheap insurance rather than a load-bearing throttle, and
  the rail can be trusted to keep up with a live crawl.
- **`page_detail` runs on every row a user opens.** The worst case is the page
  the most links point at, because the inlink *count* is not capped. On the
  100k crawl that page has **199,999 inlinks** and the pane answers in
  **6.8 ms**, returning the capped 100 of them with the true total beside it.

That second number is the cap doing its job. Without it the query would return
200,000 rows across the IPC bridge for a pane that shows a screenful — the
"UI never receives the dataset" invariant, in the one place it is easiest to
forget, because a single page does not feel like a dataset until you meet a hub.

## Repeat it

```bash
cargo build --release -p pounce-store --tests
BIN=$(find target/release/deps -name 'page_detail-*' -type f -perm +111 | head -1)
DETAIL_IN=/path/to/crawl.pounce "$BIN" time_the_results_screen --ignored --nocapture
```

It reports the busiest page it finds, and falls back to any page on a store with
no links — a seeded fixture is a legitimate thing to point it at.
