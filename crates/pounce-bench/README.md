# pounce-bench

A deterministic fixture website and a benchmark runner. Everything Pounce
claims about speed is measured here.

## Why a fixture site

Benchmarking against a real website measures the network, not the crawler, and
the result stops being reproducible the moment that site changes. This fixture
is generated from a seed, so `--seed 42 --pages 100000` produces a
byte-identical site on any machine, indefinitely.

Generation of 100k pages and 2.1M links takes ~400ms.

## Run the fixture site

```bash
cargo run --release -p pounce-bench --bin fixture-site -- --pages 100000 --seed 42 --port 8080
```

stdout carries only the base URL, so a script can capture it; diagnostics go to
stderr.

Routes beyond the generated pages:

| Route | Behaviour |
|---|---|
| `/robots.txt` | Disallows `/private/`, points at the sitemap |
| `/sitemap.xml` | Lists every indexable page, omits `noindex` ones |
| `/redirect-chain/{n}` | `n` chained 301s, terminating in a 200 |
| `/redirect-loop/{size}/{step}` | An infinite 302 loop of `size` steps |
| `/status/{code}` | Returns the requested status code |
| `/slow/{ms}` | Delays `ms` milliseconds, capped at 30s |
| `/huge/{mb}` | A page of `mb` megabytes, capped at 16 |
| `/malformed` | Unclosed tags, stray `<`, no `</html>` |

Every size and hop count is clamped. A crawler bug must not be able to exhaust
the fixture's memory or wedge it.

## Compare crawlers

```bash
cargo run --release -p pounce-bench --bin bench-runner -- \
  --pages 100000 \
  --tool  'pounce=./run-pounce.sh {url}' \
  --count 'pounce=./count-pounce.sh' \
  --tool  'freecrawl=./run-freecrawl.sh {url}' \
  --count 'freecrawl=./count-freecrawl.sh' \
  --out bench-results/run.json
```

`{url}` is replaced with the fixture's base URL. Every tool gets the same site,
the same machine, and the same measurement code — a comparison where each tool
reports its own numbers is not a comparison.

**`--count` is how a throughput figure becomes a measurement.** It runs after
the tool finishes and must print a single integer: the URLs that tool actually
crawled. The runner cannot know each tool's output format, so the operator
supplies the one-liner — `sqlite3 out.pounce 'select count(*) from pages'`, or
`jq .summary.total` over a competitor's JSON summary.

**Without `--count`, throughput and page count print as `?`.** That is
deliberate. The runner used to default the count to `--pages`, which published a
throughput number for every tool derived from the assumption that it crawled
everything it was pointed at — a tool that silently crawled half the site read
as twice as fast as it was. There is no honest fallback, so there isn't one.

Commands are split on whitespace and quoted arguments are **not** supported;
anything needing them goes in a wrapper script. Wrappers are usually needed
anyway, since most crawlers refuse to overwrite or else append to an existing
output file and each run must start clean.

Output is a JSON report plus a markdown table.

## Parse benchmarks

```bash
cargo bench -p pounce-bench
```

Baseline on the development machine, 2026-08-20:

| Benchmark | Result |
|---|---|
| `extract/lol_html` (10KB pages) | 186 MiB/s, ~19,800 pages/sec |
| `render_page` | 2.09 µs |

Run these locally on consistent hardware. CI only proves they still compile and
execute — shared runners are far too noisy for a throughput threshold to mean
anything.

## Reporting rules

Publish timed-out and failed runs rather than dropping them. A benchmark that
quietly omits the cases where a competitor struggles is not a benchmark, it is
an advertisement, and it will be found out.
