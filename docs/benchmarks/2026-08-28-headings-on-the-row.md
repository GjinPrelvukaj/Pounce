# Headings on the grid row — the join that was tried and rejected

**Date:** 2026-08-28
**Host:** Apple M5, macOS 27.0. Dev laptop, not a controlled rig.
**Harness:** `crates/pounce-store/tests/gate_m3.rs`, the M3 gate itself, seeded
at 1,000,000 rows.

```bash
cargo test --release -p pounce-store --test gate_m3 -- --ignored --nocapture
```

The Headings tab needs the first `<h1>` and how many there are. Both live in
`page_detail.h1`, a JSON array, because migration 012 moved every repeating
field out of `pages`. Two implementations were measured.

## 1. The join

```sql
SELECT ..., json_extract(d.h1, '$[0]'), json_array_length(d.h1)
FROM pages p LEFT JOIN page_detail d ON d.page_id = p.id
WHERE ... ORDER BY ... LIMIT 200 OFFSET 500000
```

`page_detail.page_id` is the primary key, so this reads as one rowid lookup per
row returned — 200 of them. It is not. SQLite computes the join for every row
`OFFSET` steps over as well.

| Unfiltered sort, 1M rows, offset 500,000 | Before | With the join |
|---|---:|---:|
| url | 11.4 ms | 66.4 ms |
| status | 7.5 ms | 24.3 ms |
| depth | 7.3 ms | 90.8 ms |
| size | 7.9 ms | 90.7 ms |
| word_count | 7.7 ms | **494.1 ms** |
| elapsed_ms | 7.5 ms | 306.1 ms |
| title | 12.4 ms | 33.6 ms |
| meta_description | 12.7 ms | 43.5 ms |

The gate is 150 ms. `word_count` missed it by 3.3x and the test failed, which
is the intended outcome: this is the shape T3.0's narrow-row split exists to
prevent, arrived at from the read side instead of the write side.

## 2. A second statement, by id

```sql
SELECT page_id, json_extract(h1, '$[0]'), json_array_length(h1), ...
FROM page_detail WHERE page_id IN (?, ?, ... 200 of them)
```

The `IN` list *is* the window, so the work is 200 primary-key lookups whether
the window came from row 0 or row 900,000.

| Unfiltered sort, 1M rows, offset 500,000 | Before | Adopted |
|---|---:|---:|
| url | 11.4 ms | 16.6 ms |
| status | 7.5 ms | 10.8 ms |
| depth | 7.3 ms | 7.3 ms |
| size | 7.9 ms | 8.0 ms |
| word_count | 7.7 ms | 8.0 ms |
| elapsed_ms | 7.5 ms | 7.7 ms |
| title | 12.4 ms | 12.5 ms |
| meta_description | 12.7 ms | 12.6 ms |

Worst filter x sort pair 149.8 ms against the 300 ms gate, memory 12 MB → 13 MB
for 200x the rows, both unchanged. Adopted.

## What this changes about the rule

CLAUDE.md said a new grid column belongs in `pages`. It still does for anything
sortable or filterable — those run against `pages` alone, and that is what the
split bought. What this adds is the third case neither document had: the *first
item of a repeating field*, shown as a column and ordered by nothing. It is
read for the window, after the window is chosen.
