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

| Fixture | Pages | Wall | URLs/s | Peak RSS | Measured |
| --- | ---: | ---: | ---: | ---: | --- |
| 10k | 10,001 | 2.02 s | 4,951 | 28 MB | 2026-08-29 |
| 100k | 100,001 | 24.35 s | 4,107 | 81 MB | 2026-08-29 |
| 500k | 500,001 | 175.3 s | 2,852 | 234 MB | 2026-08-25 |

The first two rows are the current build, **with all thirty audit rules, the
read-path index build and the sitemap comparison running**. They are *slower*
than the figures published on 2026-08-25 (1.6 s and 21.9 s), and that is the
point: today's crawl does three jobs that build did not do, for 6% and 26% more
wall time. Every URL is verified on every run — no page lost, none crawled
twice, no frontier entry left pending.

**The 500k row is older than the other two and is labelled as such.** It was
re-taken on 2026-08-29 and the run was abandoned: the machine had 12 GB of its
13 GB swap in use, and a 500,000-page crawl writing a 5.8 GB database on a
thrashing laptop produced 189 s, 218 s and 1,442 s for the same work. A
benchmark taken on a swapping machine is not a benchmark, so the older figure
stands with its date until there is an idle machine to re-take it on.

Two caveats stated rather than buried. These are localhost figures, where the
fixture answers instantly. And the rules' contribution — 13.2% at 500k — is an
artefact of exactly that: the same work is **0.22%** of a crawl against a site
with 5 ms of network latency, because the cost is a fixed ~11 µs per page and
only the denominator changes.

Memory is the number worth staring at: **RSS rose 28 → 234 MB across a 50×
increase in crawl size**, while the database on disk grew to 5.8 GB. Nothing
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
**That comparison is not current and is not quoted as a headline.** Pounce's
half was re-taken on 2026-08-29 and is in the table above; FreeCrawl's was not,
because it is no longer installed on the machine that measured it. Pairing
today's Pounce against last week's FreeCrawl would be two measurements in a
trench coat rather than a benchmark, so no ratio is published here until both
halves are taken on the same day. Benchmarks in this repository report the runs
where a tool timed out, errored, or — as above — was thrown out because the
machine was swapping.

## What it does

- Crawls of 1M+ URLs, disk-backed in SQLite from the first row. Crawls resume,
  and save as portable `.pounce` files.
- **Results appear while the crawl is still running**, because the grid queries
  the file being written rather than waiting for the end of it.
- 30 audit checks across titles, descriptions, indexability, content, response
  codes, media and links — each one reported as a sentence with a fix, not as a
  rule identifier. **And a list of what was not checked**, so a screen of green
  zeroes cannot be misread as a clean bill for checks that never ran.
- A findings rail that is also the navigation: every count opens the pages it
  counts, with the columns that finding is about.
- **Reads the site's own claims and compares them.** robots.txt is kept as
  served, the sitemap is found (from robots, or by trying the conventional
  addresses) and read, and the two disagreements are reported: URLs listed but
  reached by no link, and crawled pages listed nowhere.
- Tabs for the questions people actually ask — titles, descriptions, headings,
  duplicates, canonicals, URLs, images, response times — plus a **folder tree**
  of the site's shape, fetched a level at a time, and a search preview of how a
  page looks in results.
- Filter and sort over indexed columns, a detail pane with the full record and
  both directions of the link graph.
- **Four export formats.** An Excel workbook (summary, findings, pages, images,
  sitemap), a PDF report and an editable Word version for a client, and
  CSV/JSON of exactly the view you are looking at.
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
- **Not checked at all**: hreflang, structured data, pagination, and page speed
  beyond response time. These are named in the interface and in every exported
  report, rather than being absent and looking like a pass.
- **No link-graph visualisation** and no sitemap *generation* yet.
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
