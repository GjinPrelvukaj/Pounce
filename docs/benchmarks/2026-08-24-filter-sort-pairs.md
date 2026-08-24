# T3.2 — which filter × sort pairs get an index, measured

**Date:** 2026-08-24
**Host:** Apple M5, macOS 27.0. Dev laptop, not a controlled rig.
**Harness:** `crates/pounce-store/tests/sort_selectivity.rs`, 200,000 pages
seeded with a crawl-shaped status mix, window of 200 at offset 100,000.

```bash
cargo test --release -p pounce-store --test sort_selectivity -- --ignored --nocapture
```

---

## 1. The question

The probe showed that an indexed sort column is still 18 s when the active
filter is a *different* indexed column matching most rows. It did not say which
filters are like that. With 7 filters and 7 sorts the full cross product is 49
indices, which is not an option, so the declared set had to come from counting.

## 2. Selectivity — how much of a crawl each filter matches

| filter | rows of 200,000 | |
|---|---:|---:|
| `status = 200` | 188,000 | 94.0% |
| `kind = html` | 188,000 | 94.0% |
| `noindex = false` | 180,000 | 90.0% |
| `has_issue` | 133,333 | 66.7% |
| `depth <= 2` | 120,000 | 60.0% |
| `word_count < 300` | 44,449 | 22.2% |
| `url LIKE %/blog/%` | 40,000 | 20.0% |
| `status >= 400` | 12,000 | 6.0% |
| `noindex = true` | 20,000 | 10.0% |

**Selectivity belongs to the value, not the column.** `status = 200` matches 94%
and `status >= 400` matches 6% — same filter kind. Support is therefore declared
per *kind*, sized for that kind's unselective case, because the UI cannot know
in advance which value the user will type.

## 3. Baseline — single-column indices only (ms)

|  | url | status | depth | size | word_count | elapsed_ms | title |
|---|---:|---:|---:|---:|---:|---:|---:|
| status=200 | 100.4 | 1.4 | 78.9 | **207.5** | 172.9 | 90.2 | 101.1 |
| kind=html | 101.8 | 55.6 | 77.0 | **212.1** | 169.7 | 90.1 | 100.9 |
| noindex=false | 96.8 | 37.9 | 77.8 | **205.3** | 155.7 | 91.6 | 98.3 |
| depth<=2 | 18.6 | 9.6 | 1.4 | 27.1 | 117.2 | 92.9 | 12.0 |
| has_issue | 18.9 | 17.0 | 18.7 | 21.8 | 30.1 | 22.1 | 18.3 |

At 200k. The gate is 300 ms at **1M**, so anything here above ~60 ms is a gate
failure once the row count is five times larger. The three ~90% filters are the
problem, exactly as selectivity predicts: SQLite walks the sort index and pays a
table lookup for every row it skips.

`has_issue` is the surprise — 66.7% of rows and still 18–30 ms across every
sort. It compiles to `EXISTS` against `issues_page`, which is cheap enough that
a composite would buy nothing, so none is declared for it.

## 4. The cheaper-looking alternative, and why it is not adopted

One **sort-first covering** index per sort column — `(sort_col, status, kind,
noindex, depth, word_count)` — would be 7 indices instead of 20 and would serve
*every* filter combination rather than one declared pair. It does not work:

| status=200, sorted by | baseline | sort-first |
|---|---:|---:|
| url | 100.4 ms | **357.4 ms** |
| title | 101.1 ms | **428.2 ms** |
| size | 207.5 ms | 357.3 ms |

```
SEARCH p USING INDEX sortfirst_status (status=? AND status=?)
USE TEMP B-TREE FOR ORDER BY
```

SQLite takes the index for the **equality search** and then sorts the whole
match set in a temp B-tree — worse than having no composite at all, because it
also abandoned the sort index it was using before. Priced, refuted, recorded.

## 5. What ships — 20 declared composites

Every declared pair, same query, same offset:

|  | url | status | depth | size | word_count | elapsed_ms | title |
|---|---:|---:|---:|---:|---:|---:|---:|
| status=200 | 2.0 | 1.4 | 1.5 | 1.6 | 1.6 | 1.6 | 2.1 |
| kind=html | 2.1 | 1.5 | — | 1.6 | 1.7 | 1.6 | 2.2 |
| noindex=false | 1.9 | 1.4 | — | 1.5 | 1.6 | 1.5 | 2.0 |

A dash is a pair `SortSpec::new` refuses rather than runs. **Every declared pair
is 1.4–2.2 ms**, from 90–212 ms, and each plan names its index with no temp
B-tree:

```
SEARCH p USING COVERING INDEX pages_status_url (status=?)
SEARCH p USING INDEX pages_status_word_count (status=?)
```

**Cost: 90 MB at 200k pages** (file 291 MB → 381 MB), which extrapolates to
~450 MB at 1M and will be confirmed rather than assumed in T3.5. The ceiling is
`MAX_COMPOSITE_INDICES = 24`, asserted by a test, so the twenty-first is a
decision rather than a commit.

## 6. What is supported without an index

- A filter sorting by **its own column** — one index serves range and order.
- `has_issue` with any sort — measured above.
- `depth` and `word_count` filters with url, status, size or title sorts — all
  under 35 ms at 200k.
- `url_contains` **only** with a URL sort, where walking the URL index tests the
  pattern from the index itself. With any other sort it is a table lookup per
  skipped row, and no B-tree serves a substring match, so it is refused.

## 7. What is not established

- **200k, not 1M.** Every number here scales by extrapolation; T3.5 re-runs the
  gate at 1M through the real query layer.
- **One offset.** Half way in, which is the worst case for `OFFSET`, but the
  shallow-scroll case is untested.
- **No concurrent writer.**
- **`has_issue` at 1M** is the one supported-without-index pair with real
  headroom risk: 30 ms at 200k is ~150 ms extrapolated, inside the gate but not
  by much.

## 8. A harness bug worth keeping

The first run's "baseline" arm was the composite arm measured twice: it dropped
its indices with `LIKE 'pages_%\_%'`, and SQLite treats `_` as a wildcard unless
an `ESCAPE` clause says otherwise — so the pattern matched nothing and nothing
was dropped. The numbers looked plausible (1.9 ms) and were meaningless. The
harness now drops by name, built from the same declared list the code ships.
