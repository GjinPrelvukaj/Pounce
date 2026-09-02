# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Current state

**Audit rules live in `pounce-audit/src/rules/`, one module per themed batch**,
each registered through `rules::register_all`. Every rule ships a triggering and
a non-triggering fixture, and anything with a boundary gets mutation-checked —
break the threshold on purpose and confirm a test fails.

**M0 (benchmark harness) is built.** `pounce-bench` is complete: fixture site,
bench runner, criterion benches, CI config. **M1 (engine core) is in progress.**
Built so far: `pounce-core` (`CrawlUrl`, `Scope`, `Frontier`, bounded pipeline,
crawl lifecycle, limits, and ~10 Hz progress snapshots), `pounce-http` (robots.txt,
rate limits, retries, fetch pool, redirect chains), `pounce-parse` (`PageRecord`,
single-pass extraction, body classification), `pounce-store` (schema, batched
writer, link graph, durable frontier, redirect outcomes, terminal failures,
persisted limits, and resume state), and `pounce-cli` (minimal end-to-end
`pounce crawl`). **M2 (audit) is complete — Gate M2 closed 2026-08-23.** 30 rules across six batches, the v0.1 cap
reached and enforced by a test; rule execution measured at 5.3% of crawl wall
time against a 10% budget — but that share is an artefact of the denominator.
The rules cost a fixed **~11 µs/page**, which is 13.2% of a crawl against a
zero-latency localhost fixture and **0.22%** of one against a site with 5 ms of
delay (`docs/benchmarks/2026-08-25-rule-overhead-at-500k.md`). Quote the per-page
cost and both shares; quoting one percentage means quoting the fixture. **M3 (query layer) is complete — Gate M3 closed
2026-08-24.** `pounce-store::query` has `FilterSpec`, `SortSpec`, windowed
`query_rows` and the issue overview; the worst supported filter x sort pair is
220 ms at 1M against a 300 ms gate, down from the probe's 18,270 ms, with memory
flat at 12 MB. **M4 (desktop GUI) is complete except its last gate item — closed 2026-08-25.**
T4.1–T4.56 are done. The shape, after three rounds of owner review: **one screen,
always visible** — a persistent crawl toolbar in the header, view tabs, filter
bar, virtualised grid, a right panel with Overview and Issues tabs, and a detail
pane, all separated by draggable splitters and all rendered *before any crawl
exists*, with zeros and "No data" rather than a welcome screen. Results appear
while the crawl runs; findings read as sentences; ⌘K reaches every view, finding
and action. Measured at 500k rows: 0.00% dropped frames, 272 ms cold start,
10.2 MB peak RSS for a 149 MB export. The one open gate item is "crawl a real
site without touching a terminal", which needs a human with a mouse — see
`docs/benchmarks/2026-08-25-gate-m4.md`.

**Since 2026-08-28 it also reads the site's own claims and its shape**: a
Sitemap tab comparing what robots.txt and the sitemap declare against what the
crawl found (listed-but-unreachable, crawled-but-unlisted), a folder tree
fetched a level at a time, Headings/URLs/Duplicates tabs, a search preview in
the detail pane, and a "not checked in this version" list so a green zero
cannot be misread as a check that ran. **Four bugs found by crawling real
sites rather than the fixture** are worth knowing about, because the fixture
cannot produce any of them: a tree rooted at the seed shows nothing when a site
redirects its apex to `www`; the Images tab queried `pages` for a kind that
lives in `resources`; three counts divided findings about images by the number
of pages and printed 133% and 216%; and a sitemap declared at a redirect parsed
as an empty document. **Open a real `.pounce` file before calling UI work
done.**

**The visual system is documented, not remembered.** `DESIGN.md` (six-section
Stitch spec) and `DESIGN.json` are generated from `ui/src/index.css` and carry
ten Named Rules. Read them before any interface work; they exist because three
redesigns each aimed one layer too shallow — at wording when the problem was
layout, at layout when it was the type scale, at the type scale when it was that
the app hid itself behind a welcome screen.

**M5 is in progress**, and `PLAN.md` § M5 opens with a table of exactly what is
left and who it is blocked on. The short version: the deliverables
(T4.57–T4.60 — Excel, PDF, Word, and making Export follow the view it is on)
and the benchmark re-take need nobody; **signing and the last M4 gate item need
the owner**; the landing page needs a brand decision. `CONTRIBUTING.md` and
Sponsors were cut on 2026-08-28 — this is closed source and stays that way.
**`PLAN.md`'s first unchecked `- [ ]` is the next task — believe it over this
paragraph.** CI is green on Linux, macOS and Windows as of 2026-08-20.

Read these before doing anything substantive:

- `PLAN.md` — the master plan: every milestone, task, and gate, from empty repo to v1.0. Start here to know what to work on.
- `docs/specs/2026-08-19-pounce-design.md` — the approved design. Authoritative for architecture, scope, and branding.
- `docs/plans/2026-08-19-phase-0-benchmark-harness.md` — the Phase 0 implementation plan, 13 TDD tasks with complete code. Execute it task-by-task; do not improvise around it.
- `docs/product-plan.html` — the same design as a presentation page. Also published as an Artifact; edit this file and republish to update it in place.

## What Pounce is

A free, natively compiled technical-SEO crawler (proprietary and closed-source — see Conventions) — a Rust + Tauri desktop app with a CLI, competing against Screaming Frog (Java), FreeCrawl (Electron), and LibreCrawl (Python).

**The entire positioning is speed.** The category already has free, cross-platform, unlimited-URL competitors; the only remaining differentiator is that none of them is natively compiled. Every design tradeoff resolves toward performance, and every performance claim ships with a reproducible benchmark.

This has a direct consequence for how you work here: **a performance regression is a broken build, not a cleanup task.**

## Commands

```bash
cargo build                                    # whole workspace
cargo test --workspace --lib --bins --tests -- --test-threads=1   # what CI runs
cargo test --workspace --all-targets           # also fine, but only with NO `-- args`
cargo test -p pounce-bench --test graph_properties   # one test file
cargo test -p pounce-bench rng                 # one module's inline tests
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo bench -p pounce-bench                    # criterion benchmarks
cargo bench -p pounce-bench -- --test          # compile-and-run benches without sampling
cargo bench -p pounce-bench --bench store      # insert throughput
UPDATE_GOLDEN=1 cargo test -p pounce-parse     # regenerate the extraction goldens
```

The desktop app (`crates/pounce-app` + `ui/`):

```bash
npm --prefix ui run dev                  # terminal 1: Vite on :1420
cargo run -p pounce-app                  # terminal 2: the app, against that dev server
POUNCE_DEVTOOLS=1 cargo run -p pounce-app   # same, with the inspector open
npm --prefix ui run build                # typecheck (tsc -b) + bundle into ui/dist
npm --prefix ui run check:contrast       # every text tier against its theme, AA

POUNCE_STARTUP=1 ./target/release/pounce-app   # prints cold start to the first paint

# The production path — the ONLY one that uses the bundled ui/dist:
cd crates/pounce-app && ../../ui/node_modules/.bin/tauri build --no-bundle
```

**`cargo build` always loads `devUrl`, in release too.** Only a build through
the Tauri CLI is a "prod" build that embeds `ui/dist`; `cargo run` and
`cargo run --release` both point the webview at localhost:1420, so without Vite
running they give a white window and a DOM of
`<html><head></head><body></body>` — a *failed navigation*, not a render error.
An earlier commit claimed the release binary had been verified against
`ui/dist`; it had not — see the pkill gotcha below for why the test lied.

**The Tauri CLI runs `beforeBuildCommand` from `Pounce/crates`**, not from the
directory holding `tauri.conf.json`, which is why those commands say
`--prefix ../ui`. Verify with `pwd` as the command rather than reasoning about
it; it is not the documented-looking answer.

Running the fixture site and benchmarks:

```bash
cargo run --release -p pounce-bench --bin fixture-site -- --pages 100000 --seed 42 --port 8080

cargo run --release -p pounce-bench --bin bench-runner -- \
  --pages 100000 \
  --tool 'pounce=./target/release/pounce crawl {url} --quiet' \
  --out bench-results/run.json
```

Benchmarks must be run under `--release`. Debug-build numbers are meaningless and must never be published.

## Architecture

A Cargo workspace of small crates. The GUI is one consumer of the engine, never its owner — the CLI and the desktop app are peers over the same core.

```
pounce-core      orchestrator, frontier, scheduler, crawl lifecycle
pounce-http      fetch pool, retries, redirect chains, robots.txt, rate limits
pounce-parse     lol_html streaming extraction → PageRecord (depends on pounce-http)
pounce-store     SQLite schema, batched writer, query API, resume state
pounce-audit     rule registry; each check is one testable unit
pounce-export    CSV / JSON, streamed; XLSX and sitemap XML not built
pounce-run       the crawl runner: frontier loop, image pass, lifecycle wiring
pounce-bench     fixture site + benchmark runner (Phase 0, built first)
pounce-cli       headless binary, CI exit codes
pounce-app       Tauri commands + events (thin)
ui/              React + TanStack, virtualised grid
```

The pipeline, with bounded channels between every stage so a slow disk throttles the fetchers instead of ballooning memory:

```
frontier → fetch pool → parse → batched writer (~500/txn) → audit rules
```

## Invariants

These are decisions already made and paid for. Changing one means changing the spec first, not the code.

**The UI never receives the crawl dataset.** It sends queries; SQLite returns only the visible window (~200 rows). Sorting and filtering are SQL against indexed columns; scroll position maps to `OFFSET`. The intuitive alternative — crawl to memory, serialise over Tauri IPC, render in React — collapses around 100k rows and would make a Rust app feel slower than the Electron competitor. This is documented as the single most likely way the project fails.

**Progress events are throttled to ~10 Hz.** Never one event per crawled URL.

**Storage is disk-backed from the first row.** No in-memory-then-dump mode. Resumable crawls and portable `.pounce` files depend on it.

**Auto-redirect is disabled in reqwest.** The redirect chain is data the crawler must record, not plumbing to be followed transparently.

**Fixture determinism must not depend on third-party crates.** `pounce-bench` uses a hand-rolled SplitMix64 (`rng.rs`), not `rand` — a fixture site that reshapes itself on a dependency bump would invalidate every historical benchmark. `rng.rs` has a known-answer test; if it fails, treat it as a breaking change rather than updating the expectation.

**`CrawlUrl`'s serde is hand-written, never derived.** Deserialisation re-parses,
so a hand-edited `.pounce` file cannot produce a `CrawlUrl` that skipped
validation. The type's whole value is that holding one proves the checks ran.

**Absent is not empty, anywhere in `PageRecord`.** A missing `<title>` and
`<title></title>` are different findings, as are a missing `alt` and `alt=""`.
`Option<String>` carries that distinction all the way into SQL as NULL vs `''`.

**`pages` is the narrow grid row; the six repeating JSON fields live in
`page_detail`.** Split by migration 012 after T3.0 measured it: the extra insert
per page made the write path 2.65% *faster*, not slower, because those bytes
stop being dragged through a B-tree carrying eight indices. The duplicated
`row_view` alternative was measured and rejected. A new grid column belongs in
`pages`; anything the detail pane alone reads belongs in `page_detail`. The
third case, added by T4.47: the *first item of a repeating field* shown as a
column and ordered by nothing — read for the window by a second statement over
`page_detail`, keyed by the ids just returned. Not a `LEFT JOIN`, which SQLite
computes for every row `OFFSET` skips as well and which took an unfiltered 1M
sort from 7.7 ms to 494 ms
(`docs/benchmarks/2026-08-28-headings-on-the-row.md`).

**`pages.has_issue` is a cache, and is checked like one.** The grid's most-used
filter was an `EXISTS` costing one subquery per row the offset skipped, so it got
slower the further you scrolled — 220 ms at 1M against 15 ms for every other
filter. Migration 013 denormalises it so the filter is an equality the composites
serve (11–14 ms), at ~0.5% of crawl wall time. `issues` stays the source of
truth: `build_query_indices` rebuilds the column, and a test fails if one page
disagrees with the table. Do not add a second such column without the same three
things — a rebuild, a test, and a measured reason.

**Pages are upserted on `url`, never `INSERT OR REPLACE`.** Replace changes the
row `id` and orphans every link edge pointing at it. Store tables are `STRICT`:
a status stored as text sorts as text, and the grid would put 99 after 100.

**Content is classified, never sniffed into.** Magic bytes may *contradict* a
declared `Content-Type` (setting `content_type_mismatch`) but never override it —
silently trusting the bytes hides the server misconfiguration that is the finding.
A response with no declared type is reported untyped, not guessed at.

**Politeness defaults are correctness, not configuration.** robots.txt honoured by default, per-host concurrency caps, `Retry-After` respected, honest user-agent carrying a project URL. Aggressive settings are opt-in and clearly labelled. A fast crawler that gets its users IP-banned is a liability.

**v0.1 caps at ~30 audit rules.** Feature parity with FreeCrawl's 200+ checks is explicitly not the goal. Racing a feature list we are behind on is the identified primary failure mode. New rules land after the rule SDK exists, and then they are the community's job.

**Every audit rule ships with two tests** — one fixture that triggers it, one that does not. This is what stops rule count becoming rule debt.

**Benchmarks report failures.** Timed-out and errored runs appear in the published table rather than being dropped. A benchmark that omits the cases where a competitor struggles is an advertisement.

## Design tokens

**`PRODUCT.md` § Brand Commitments is authoritative; the spec's §3 palette is superseded.** Since the 2026-08-25 redesign the accent is violet (`#6A4DF4`; `#5A3FD6` light / `#AC9BFF` dark as text) over a **warm** neutral stack — paper and charcoal, not the earlier cool blue-black. Inter everywhere with `.nums` for aligned figures; JetBrains Mono only for text read character by character. The constraint that has survived both redesigns: a crawler UI is dominated by severity states, so **the brand colour must never move into the red–amber–green band**, and severity must never be encoded by colour alone — always pair with an icon and a label. `npm run check:contrast` enforces AA on every tier in both themes and fails the build otherwise.

## Gotchas

- **Never combine `--all-targets` with `-- <test-harness args>`.** `--all-targets`
  sweeps in the criterion benches, whose CLI rejects `--test-threads` and exits 2.
  This failed CI four times. Use `--lib --bins --tests` whenever passing `--` args.
- **`pkill -f "a\|b"` matches nothing.** `pkill` uses ERE, where `\|` is a
  literal, so an alternation written that way kills no process and reports
  success. A "release build with the dev server stopped" test passed for
  exactly this reason while Vite was still serving the page. Use separate
  `pkill` calls, and confirm with `pgrep -fl` before trusting the result.
- **Screenshotting the app needs the window id, not the screen.** The window is
  often behind whatever launched it, so a full-screen capture catches the wrong
  thing. `swift` can list windows via `CGWindowListCopyWindowInfo` (system
  python has no `Quartz` module), then `screencapture -x -o -l <id> out.png`
  grabs that window whatever its stacking order.
- **A rAF delta is one frame interval, not a latency.** At a locked 60 Hz vsync
  every healthy frame measures ~16.7 ms, so "frames slower than 16.7 ms" counts
  good frames as bad — the first grid measurement read 57% and meant nothing.
  Sample an idle baseline first, then count *dropped* frames (> 1.5× baseline).
  And measure the grid on a production build: debug SQLite behind every window
  fetch took p95 from 23 ms to 37 ms.
- **Pause has to be gated at the fetch stage, not just at admission.**
  `admit()` runs a whole bounded channel ahead of the fetch pool, so ~80 URLs
  are already accepted when the button is pressed. That is 0.2 s on a fast
  crawl and most of a minute on a polite one — which is the crawl someone is
  most likely pausing *because* of. `CrawlLifecycle::wait_while_paused` is
  awaited immediately before each request, leaving only in-flight fetches.
- **A crawl that fails must end its lifecycle.** `report_progress` runs until
  the status is terminal, so an error path that leaves it `Running` ticks
  forever and whoever awaits the reporter waits forever — the command never
  returns. `CrawlLifecycle::fail()` exists for that; the symptom was a progress
  line reading "running · 0 written" beside a finished crawl.
- **A batch is not a crawl.** `run_controlled_pipeline` deliberately does *not*
  call `lifecycle.complete()` — the runner feeds it bounded slices of the
  frontier, and completing per slice left the second one admitted against a
  terminal status, crawling nothing. Whoever owns the loop owns that call.
- **A bundled `.app` is subject to macOS TCC; a bare binary is not.** Run from a
  terminal, the binary inherits the terminal's permissions. Launched as
  `Pounce.app`, opening a `.pounce` file in `~/Documents` raises a folder-access
  prompt, and until it is answered the window paints its background and nothing
  else — indistinguishable from the blank-webview failure below. The prompt can
  be behind another window. `screencapture -x` of the whole screen finds it; the
  same file under `/tmp` opens instantly. Killing the app does not dismiss the
  dialog (`killall UserNotificationCenter` does, without granting anything).
- **macOS stops painting an occluded window.** A `screencapture -l` of a Tauri
  window that is fully covered returns its background colour and nothing else,
  which looks exactly like a render bug. Capture right after launch while the
  window has focus. If the *display* is asleep the capture fails outright with
  "could not create image from window" — `caffeinate -u -t 10` wakes it.
- **The frontend in a browser has no engine.** `npm run dev` serves the UI over
  http, where `window.__TAURI_INTERNALS__` does not exist and every `invoke`
  throws `TypeError: Cannot read properties of undefined`. That path is fine for
  layout, CSS and themes — and it is the only way to see the UI without screen
  capture — but the bridge can only be exercised by `cargo run -p pounce-app`.
  `ui/src/engine.ts` detects it and says so rather than surfacing the TypeError.
- **Tauri needs `crates/pounce-app/icons/icon.png` to compile**, not just to
  bundle: `generate_context!` reads it at macro expansion and the build fails
  without it. Bundling itself is off until M5.
- **`ls` on this machine opens a pager and hangs the Bash tool.** Use `ls -la`,
  `find`, or `git ls-files`.
- **An anchor-matched edit that finds no match silently does nothing.** A
  `replace(old, new)` whose `old` has drifted — often because `cargo fmt`
  reflowed it — leaves the file untouched and the command still exits 0. This
  shipped a commit without its `PLAN.md` update. Assert the match, or check the
  diff before committing.
- **The Bash tool's cwd persists across calls.** A `cd` in one call silently
  changes where the next one's relative paths resolve — a `PLAN.md` edit failed
  that way and the commit went out without it. Prefer absolute paths.
- **lol_html 3.0:** `Settings::new()` is a builder (0.x's public fields are gone);
  `el.on_end_tag` needs the `end_tag!` macro, not a bare closure; and **text
  handlers return raw source text — entities are not decoded** (hence `html-escape`).
- **`<title>` is RCDATA.** Tags after an unclosed `<title>` are *text*, not
  elements. A test expecting otherwise is testing something HTML cannot do.
- **Golden JSON is compared semantically, not as raw text.** Git may check it
  out with CRLF on Windows while serde emits LF; byte comparison makes equal
  records fail only on Windows.
- **`CREATE TABLE x AS SELECT ...` gives `id` as an ordinary column, not the
  rowid.** A composite index on such a table has no implicit id tail, so an
  `ORDER BY col, id` tie-break falls back to `USE TEMP B-TREE FOR LAST TERM OF
  ORDER BY`. Declare `id INTEGER PRIMARY KEY` and fill with `INSERT ... SELECT`.
- **A composite `(filter, sort)` index only works for an *equality* filter.**
  `depth = 2` sorted by size is 1.6 ms with one and 282 ms without; `depth <= 2`
  is 101 ms either way, because a range leaves SQLite walking the sort index and
  fetching each row to test the filter. Support is declared per `FilterShape`,
  not per column — and a range is refused the low-cardinality sort columns
  (`word_count`, `elapsed_ms`) where that per-row lookup costs 450 ms at 1M.
- **`_` is a wildcard in `LIKE`.** `DROP INDEX ... LIKE 'pages_%\_%'` matches
  nothing without an `ESCAPE` clause, which is how a benchmark's "baseline" arm
  was quietly the composite arm measured twice. Drop by name.
- **`count(DISTINCT x)` ignores NULLs.** Counting distinct `page_id` in `issues`
  silently omits every finding whose subject never became a page — a redirect
  loop, an unreachable host. Those need their own query.
- **URLs are stored canonical, so a search needle must be encoded too.** `/café/`
  is on disk as `/caf%C3%A9/`; a raw needle matches nothing and an empty grid
  looks like an answer. IDN *hosts* are punycoded and remain a known gap.
- **An index that exists is not an index that is *built yet*.** `links_target`
  is deferred to `build_query_indices()`, so any query joining on
  `links.target_url` before that point silently gets a full table scan per row.
  This cost `links.orphan-page` **45 s on a 10k store** until the index build
  was split into `build_link_index()` and moved ahead of the site rules. Assert
  the query plan, never assume it.
- **`EXPLAIN QUERY PLAN` says `USING COVERING INDEX`**, not just `USING INDEX` —
  an index assertion matching only the latter gives a false failure.
- **axum 0.8 uses `{param}`, not `:param`.** The 0.7 colon syntax panics at router construction.
- **`pounce` and `pounce-cli` are taken on crates.io** (an unrelated chess engine). All other `pounce-*` names are free. The CLI can publish as `pounce-seo` while installing a binary named `pounce`.
- **Fixture pages live at `/{section}/{word}-{id}`, not `/page/{n}`.** Paths are generated, so read them from `graph.nodes[i].path` rather than inventing one — an invented path is a silent 404, and a benchmark written against one measures the fixture's 404 handler.
- **The workspace `reqwest` sets `default-features = false`.** Anything you assume is on may not be; HTTP/2 was silently off until T1.6. Check the feature list before relying on a capability. `zstd` stays off deliberately — `zstd-sys` is a C build.
- **sysinfo's `refresh_processes_specifics` signature changes between versions**, and `Process::memory()` returns bytes (not KB) since 0.30.
- Crates stay `publish = false` until there is something worth releasing.

## Conventions

- Conventional commits (`feat(bench):`, `docs:`, `ci:`).
- **Proprietary and closed-source.** `LICENSE` is all-rights-reserved. Crates set `publish = false` and carry no `license` field — nothing goes to crates.io. Do not add open-source licence headers, contributor docs, or public-community furniture.
- Plans live in `docs/plans/`, specs in `docs/specs/`, both dated `YYYY-MM-DD-`.
