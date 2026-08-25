# Pounce

A technical SEO crawler that is natively compiled. Crawl a site, see every
defect it has, and keep the whole crawl in a file you can reopen — at a scale
where the tools people use today start to struggle.

The category already has free, cross-platform, unlimited-URL competitors.
**None of them is natively compiled**: Screaming Frog runs on the JVM, FreeCrawl
is Electron and Node, LibreCrawl is Python and Flask. Each pays a runtime tax on
the two things a crawler does most — parsing HTML and holding a large result set
— and none can shed it without a rewrite. That is the whole claim, and it is
narrow on purpose.

## Numbers

Apple M5, macOS 27, `--release`, against `pounce-bench`'s deterministic fixture
site on localhost. A dev laptop, not a controlled rig. Every figure below has a
file in [`docs/benchmarks/`](docs/benchmarks/) with the command that produced it.

| Fixture | Pages | Wall | URLs/s | Peak RSS |
| --- | ---: | ---: | ---: | ---: |
| 10k | 10,001 | 1.6 s | 6,251 | 25 MB |
| 100k | 100,001 | 21.9 s | 4,566 | 71 MB |
| 500k | 500,001 | 175.3 s | 2,852 | 234 MB |

Re-measured 2026-08-25 on the current build, **with all thirty audit rules
running** — the earlier published figures (17.8 s at 100k, 133.9 s at 500k)
predate the rules entirely and are not comparable. Every URL is verified: no
page lost, none crawled twice, no frontier entry left pending.

Two caveats stated rather than buried. The machine had been building and
crawling for hours when these were taken and measures about 8% slower than an
idle one. And the rules' contribution here — 13.2% at 500k — is an artefact of
crawling a localhost fixture that answers instantly: the same work is **0.22%**
of a crawl against a site with 5 ms of network latency, because the cost is a
fixed ~11 µs per page and only the denominator changes.

Memory is the number worth staring at: **RSS rose 25 → 234 MB across a 50×
increase in crawl size**, while the database on disk grew past 5 GB. Nothing
accumulates in proportion to the crawl.

The interface holds the same line. At 500,000 rows the grid drops **0.00%** of
frames over six seconds of continuous scrolling (baseline 17.0 ms, worst
19.0 ms), and the app cold-starts in **272 ms** (median of thirteen launches).
Exporting all 500,000 rows to a 149 MB JSON file peaks at **10.2 MB** of
resident memory.

### Against other tools

A 10,000-page head-to-head against FreeCrawl 0.9.6, given its best
configuration, measured Pounce ~49× faster on ~26× less memory
([`2026-08-21-freecrawl-head-to-head-10k.md`](docs/benchmarks/2026-08-21-freecrawl-head-to-head-10k.md)).
**That comparison is not current and is not being quoted as a headline.**
Pounce got 1.43× faster at 10k after it was taken, and the fixture has changed
shape since; the competitor's own numbers stand, but the ratio needs re-taking
on one fixture before it is published anywhere. Benchmarks here report the runs
where a tool timed out or errored, rather than dropping them.

## What it does

- Crawls of 1M+ URLs, disk-backed in SQLite from the first row. Crawls resume,
  and save as portable `.pounce` files.
- **Results appear while the crawl is still running**, because the grid queries
  the file being written rather than waiting for the end of it.
- 30 audit checks across titles, descriptions, indexability, content, response
  codes, media and links — each one reported as a sentence with a fix, not as a
  rule identifier.
- A findings rail that is also the navigation: every count opens the pages it
  counts.
- Filter and sort over indexed columns, a detail pane with the full record and
  both directions of the link graph, and CSV/JSON export of exactly the view
  you are looking at.
- Windows, macOS and Linux from one codebase; a desktop app and a CLI as peers
  over the same engine.

## What it does not do yet

Stated plainly, because a feature list that quietly omits its gaps is an
advertisement:

- **No JavaScript rendering.** A page whose links only exist after JS runs is a
  page Pounce currently sees empty. Planned for v0.3, opt-in, with the cost
  stated in the interface.
- **No Search Console or GA4 integration**, no scheduled crawls, no log-file
  analysis.
- **No custom extraction** (CSS/XPath/regex) and **no crawl-to-crawl diffing**
  yet. Both are planned; neither exists.
- **30 rules, not 200.** Feature parity with the longest checklist is explicitly
  not the goal — racing a list we are behind on is the identified way this
  project fails. New rules land after the rule SDK exists.
- **No link-graph visualisation**, no XLSX export, no sitemap generation yet.
- **IDN hosts are a known gap**: URLs are stored canonical and punycoded, and a
  search needle typed in Unicode will not match one.
- **Installers are not signed or notarised yet**, so the first launch will need
  the usual per-platform override.

## Building it

Rust (edition 2024, 1.97+) and Node for the interface.

```bash
cargo test --workspace --lib --bins --tests -- --test-threads=1   # what CI runs
cargo clippy --workspace --all-targets -- -D warnings
npm --prefix ui run build
```

The desktop app in development — two terminals:

```bash
npm --prefix ui run dev            # Vite on :1420
cargo run -p pounce-app            # the app, against that dev server
```

`cargo build` always loads the dev server, in release too. The only build that
embeds the bundled interface is one through the Tauri CLI:

```bash
cd crates/pounce-app && ../../ui/node_modules/.bin/tauri build --no-bundle
```

Benchmarks must be run under `--release`; debug numbers are meaningless and are
never published.

```bash
cargo run --release -p pounce-bench --bin fixture-site -- --pages 100000 --seed 42 --port 8080
cargo run --release -p pounce-bench --bin bench-runner -- \
  --pages 100000 --tool 'pounce=./target/release/pounce crawl {url} --quiet' \
  --out bench-results/run.json
```

More in [`ARCHITECTURE.md`](ARCHITECTURE.md), which starts with the one decision
everything else follows from.

## Licence

Proprietary. Copyright © 2026 Gjin Prelvukaj, all rights reserved — see
[`LICENSE`](LICENSE). Nothing here is published to crates.io; every crate sets
`publish = false`.
