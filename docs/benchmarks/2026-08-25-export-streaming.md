# Export at 500k rows — the streaming claim, measured

**Date:** 2026-08-25
**Build:** `--release`, the test binary directly under `/usr/bin/time -l` so the
number is the export's own peak resident set and not a test harness's.
**Input:** `/tmp/big.pounce`, 500,000 pages, 681 MB on disk.

| Format | Rows | Output | Wall | Peak RSS |
| --- | --- | --- | --- | --- |
| CSV | 500,000 | 77 MB | 1.34 s | **10.2 MB** |
| JSON | 500,000 | 149 MB | 1.54 s | **10.2 MB** |

## Why this is the number that matters

T4.14 claims exports "never materialise in memory". That is true by
construction — one prepared statement, one `BufWriter`, one row alive between
them — but this project's rule is that a claim about performance ships with a
measurement. If the rows accumulated, peak RSS would track the output: 77 MB for
the CSV, 149 MB for the JSON. It is **10.2 MB for both**, and identical across
two formats whose outputs differ by a factor of two. That is the shape of a
stream.

Throughput is 373,000 rows/s to CSV and 324,000 rows/s to JSON, which is
disk-and-serialiser bound rather than query bound; the underlying scan is one
`SELECT` over `pages` with no join, and the twelve exported columns are all
scalars on that table by design.

Both outputs were checked for correctness afterwards, not just for size: the CSV
header and first row parse as expected, and all 500,000 JSON rows load in a
single `json.load`.

## How to repeat it

```bash
SEED_OUT=/tmp/big.pounce SEED_PAGES=500000 \
  cargo test --release -p pounce-store --test gate_m3 seed_a_store -- --ignored --nocapture

cargo build --release -p pounce-export --tests
BIN=$(find target/release/deps -name 'export-*' -type f -perm +111 | head -1)
SEED_IN=/tmp/big.pounce SEED_OUT=/tmp/big.csv /usr/bin/time -l "$BIN" \
  stream_a_seeded_store --ignored --nocapture
```

`SEED_OUT` ending in `.json` writes JSON instead. Run the binary directly rather
than through `cargo test`: `cargo` is the parent process, and `/usr/bin/time`
would be measuring it.
