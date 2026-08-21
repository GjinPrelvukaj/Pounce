# Pounce — Master Plan

The full build, milestone by milestone and task by task, from empty repo to v1.0.

**How this file relates to the others**

| Document | Scope |
|---|---|
| `docs/specs/2026-08-19-pounce-design.md` | *What* we're building and why. Authoritative for architecture and scope. |
| **`PLAN.md`** (this file) | *In what order.* Every milestone, every task, every gate. |
| `docs/plans/YYYY-MM-DD-*.md` | *Exactly how,* for one milestone. Full TDD code. Written just-in-time. |

Only M0 has a detailed plan today. **Write each milestone's detailed plan when you reach it, not before** — Phase 0's measurements will invalidate guesses made now.

**Status (2026-08-20):** M0 built — 13 tasks, 80 tests, harness usable end to end. The competitor baseline is deferred to Gate M1 by decision; see Gate M0. **Now in M1 — engine core**, whose gate is the go/no-go on the whole premise.

---

## Milestones at a glance

| # | Milestone | Ships | Est. | Gate |
|---|---|---|---|---|
| **M0** | Benchmark harness | built ✅ | 2 wks | Harness measures any crawler reproducibly |
| **M1** | Engine core | — | 5–7 wks | **GO/NO-GO: is Pounce actually faster?** |
| **M2** | Audit engine | — | 2–3 wks | 30 rules, each with passing + failing fixtures |
| **M3** | Query layer | — | 1–2 wks | **500k-row sort under 150ms** |
| **M4** | Desktop GUI | — | 5–7 wks | A crawl is startable, browsable, exportable |
| **M5** | **MVP release** | **v0.1** | 2 wks | **Signed builds on 3 platforms + published benchmark** |
| M6 | CLI & CI | v0.2 | 6 wks | GitHub Action fails a build on regression |
| M7 | JavaScript rendering | v0.3 | 8 wks | JS-dependent links discovered on the fixture |
| M8 | Extraction & diffing | v0.4 | 6 wks | Crawl-to-crawl diff of a changed fixture |
| M9 | Integrations & scale | v0.5 | 8 wks | 10M-URL crawl completes |
| M10 | Rule SDK | v1.0 | — | A third-party rule loads and runs |

**Total to MVP: roughly 17–23 weeks of solo work.** Treat that as a planning figure, not a promise; M1 is the one most likely to overrun.

Two gates are load-bearing and exist to kill the project cheaply if the premise is wrong: **M1** (are we actually faster?) and **M3** (does the architecture hold at scale?). Do not soften either one.

---

## M0 — Benchmark harness

**Goal:** measure any crawler, reproducibly, before writing a crawler.
**Detailed plan:** [`docs/plans/2026-08-19-phase-0-benchmark-harness.md`](docs/plans/2026-08-19-phase-0-benchmark-harness.md) — full TDD code for all 13 tasks.

- [x] **T0.1** Cargo workspace, pinned toolchain, `.gitignore`
- [x] **T0.2** Deterministic SplitMix64 PRNG with a known-answer test
- [x] **T0.3** Site graph generation — spanning tree, nav, cross-links, seeded SEO defects
- [x] **T0.4** HTML rendering of graph nodes
- [x] **T0.5** Pathological cases — redirect chains/loops, slow, huge, malformed
- [x] **T0.6** axum server with hash-lookup fallback routing, robots.txt, sitemap.xml
- [x] **T0.7** `fixture-site` binary
- [x] **T0.8** Child-process wall-time and peak-RSS sampler
- [x] **T0.9** `BenchResult` / `Report` types, markdown + JSON output
- [x] **T0.10** `bench-runner` binary comparing arbitrary crawlers
- [x] **T0.11** Criterion parse and render benchmarks
- [x] **T0.12** CI: fmt, clippy, cross-platform tests, bench smoke
- [x] **T0.13** `pounce-bench/README.md`

**Gate M0** — all true before starting M1:
- [x] `fixture-site --pages 100000` generates and serves in under 5s — **408ms** for 100k pages / 2.1M links
- [x] Parse throughput baseline recorded — **186 MiB/s, ~19,800 pages/sec** (`crates/pounce-bench/README.md`)
- [x] `bench-runner` produces a table and JSON — verified with curl at 12,195 URL/s
- [x] Fixture serving ceiling established — **~17,000 req/s**, so the harness never bottlenecks a crawler under test
- [x] `cargo test --workspace` green on Linux, macOS, Windows — CI first ran green on all three on 2026-08-20 (run 32397434086). The first four runs failed: `--all-targets` swept the criterion benches into `cargo test`, and `-- --test-threads=1` is forwarded to every target, which criterion's CLI rejects. CI now runs `--lib --bins --tests`; the bench-smoke job still covers the benches.
- [~] **Competitor benchmarked on a 100k fixture** — **deliberately deferred to Gate M1 on 2026-08-20**

A preliminary FreeCrawl probe (5k pages, one configuration) is recorded in
[`docs/benchmarks/2026-08-20-fixture-ceiling-and-freecrawl-probe.md`](docs/benchmarks/2026-08-20-fixture-ceiling-and-freecrawl-probe.md):
28 URL/s, 435 MB peak, with long GC-shaped stalls. It used `--concurrency 200`
against a default of 20, so the number is indicative only, not a fair baseline.

Deferred rather than dropped: a full 100k run costs 30–60 min per configuration,
and the head-to-head that actually decides the premise belongs at Gate M1, where
both tools can be measured in the same session. **Gate M1 still requires it.**

---

## M1 — Engine core

**Goal:** a headless crawl of the 100k fixture, end to end, writing to SQLite. No GUI, no audit rules yet.
**Why before the GUI:** it is the only way to answer the speed question, and per the risk table, something working at week 6 is what sustains momentum through the Tauri learning curve.

### URL handling — `pounce-core`

- [x] **T1.1** `Url` newtype with normalisation: relative resolution, trailing slash, case, default ports, fragment stripping, punycode
  *Done when:* property tests cover each transform and idempotence (`normalise(normalise(u)) == normalise(u)`)
- [x] **T1.2** Internal/external classification, subdomain policy, `nofollow` handling
  *Done when:* fixture links classify correctly against a configured host
  *Deviation:* subdomain matching is against the **seed host**, not the
  registrable domain — no public suffix list. `www`/apex are treated as one
  host, which covers the common case; a seed at `blog.example.com` does not
  reach `shop.example.com`. Marked `ponytail:` in `scope.rs`; add `psl` if a
  user hits it.
  *Fixture gap:* the fixture site emits **no external links and no
  `rel="nofollow"`**, so classification is tested against a host table plus
  resolved fixture paths, not against real fixture cross-origin links. M2's
  mixed-content and broken-external-link rules will need those links added —
  which changes rendered page bytes and so shifts the parse-bench baseline.

### Politeness — `pounce-http`

- [x] **T1.3** robots.txt fetch, parse, and cache per host — wildcards, `Allow` precedence, `Crawl-delay`, malformed input
  *Done when:* an edge-case fixture suite passes; a disallowed path is never fetched
  *Dependency:* parsing and matching come from **`robotxt` 0.6** rather than
  hand-rolled. Longest-match `Allow` precedence, `*`/`$` wildcards, user-agent
  group selection and RFC 9309 §2.3.1 access semantics are a few hundred lines
  of subtle rules, and getting them wrong is a politeness failure. The 15
  parse-behaviour tests are an acceptance suite over the dependency: they fail
  loudly if it changes or is swapped.
  *Scope:* `RobotsCache` is keyed by **origin**, holds one parsed file per
  origin, and walks robots.txt redirects itself (auto-redirect stays disabled).
  `is_allowed` is the hot path; `get` exposes `crawl_delay` for T1.4.
  *Not yet proven:* "a disallowed path is never fetched" is asserted at the
  `is_allowed` boundary. The end-to-end version of that claim belongs to T1.17,
  once a pipeline exists to observe.
- [x] **T1.4** Per-host rate limiter and concurrency caps
  *Done when:* a 10 req/s cap is observed over a 30s fixture run — **measured
  2026-08-20: 300 requests in 30s = 10.00 req/s at a 10/s cap**, and with the
  interval removed, peak in-flight 4 against a cap of 4 over 35,373 requests.
  Both in `crates/pounce-http/tests/politeness.rs`, `--ignored`. (Re-measured
  during T1.6: the first run of these requested `/page/{n}`, which the fixture
  does not serve, so it had been timing its 404 handler. The rate figure was
  unaffected — a 404 is still a round trip — but the throughput one was, and
  both now fetch real generated pages.)
  *Deviation:* **`governor` was not used.** Its default features pull ten
  crates (dashmap, quanta, parking_lot, rand, getrandom, futures-*) to provide
  GCRA, whose burst allowance is the wrong shape for politeness anyway — a host
  that has been quiet for a minute should not be able to absorb sixty requests
  at once. An even minimum interval is what `Crawl-delay` means, and it is a
  `Mutex<Instant>` plus `sleep_until` over the tokio already in the tree.
  *Shape:* `Limiter::acquire(host, min_interval)` takes the interval per call
  rather than storing it, because the effective value is whatever robots.txt
  said and the caller already has it from `RobotsCache::get`. Nothing to
  invalidate. Keyed by **host**, not origin — a rate limit protects a server,
  and http/https on one name are the same server.
- [x] **T1.5** Identifiable user-agent, `Retry-After` handling, retry with backoff
  *Shape:* `pounce_http::client()` is now the one place the two client-wide
  invariants are stated — the user-agent, and auto-redirect being off. Test
  files that used to hand-roll `Policy::none()` go through it, so forgetting it
  is no longer possible at a call site.
  *Retry policy:* only 429/502/503/504 and transport errors retry. **500 is
  deliberately not retried** — a page that errors is an audit finding, and
  retrying it three times triples the cost of crawling a broken site.
  `Retry-After` is honoured in both RFC 9110 forms (delta-seconds and
  HTTP-date, the latter via `httpdate`, already in the tree under hyper), never
  earlier than our own backoff, and **refused entirely past 60s** rather than
  parking a fetch slot for an hour.
  *Deferred to T1.6, on purpose:* the retry **loop**. `RetryPolicy` decides
  whether and how long; the pool owns the attempt counter — and must send each
  retry **back through `Limiter::acquire`**, since a retry is another request
  to a host that just asked for room.
  *Also T1.6:* a per-request timeout. `client()` sets none, so a hung
  connection would park a worker indefinitely.
  *Open:* `USER_AGENT` points at a private repository, so the URL 404s for
  anyone who follows it. Marked `TODO` in `lib.rs`; must be a public page
  before Pounce crawls any site we do not own.

### Fetching — `pounce-http`

- [x] **T1.6** Fetch pool over `reqwest` with pooling and compression, **auto-redirect disabled**
  *Mostly already true:* reqwest pools connections by default and
  auto-redirect was disabled in `client()` at T1.5. What this task actually
  added is the assembly — `Fetcher::fetch` applies robots, then the limiter,
  then the retry loop, and nothing else in Pounce sends a request.
  *Client features:* added `http2` (the spec promised it; it had been off,
  since the workspace dep sets `default-features = false`) and `brotli`.
  **`zstd` deliberately not enabled** — `zstd-sys` is a C build, which would
  make a C toolchain a prerequisite on all three platforms for an encoding
  still rarely served.
  *Errors:* a failing status is not an error. 404, and a 503 that outlived its
  retries, both return `Ok` — they are findings the audit must record. Only a
  request that produced no response is `Err`.
  *Absorbed from T1.8:* the body is read inside the fetcher, so `Fetched`
  already carries status, headers, body, elapsed, and a `truncated` flag. This
  was forced rather than chosen: the host's concurrency permit has to cover the
  body read, and a permit released before the body is drained is not a
  concurrency limit. T1.8 is therefore mostly done; what remains there is the
  reporting shape, not the capture.
  *Also absorbed:* the oversized-body cap that T1.11 lists. Content-type
  handling still belongs to T1.11.
  *Bug found by the integration tests:* `RetryPolicy` was off by one — with
  `max_attempts: 3` it sent **four** requests. The unit tests had encoded the
  same misreading, so only a test that counted requests arriving at a real
  server caught it. Fixed in `retry.rs`; the unit tests now assert the limit is
  a total, not a retry count.
- [x] **T1.6a** Reporting gap: an unreachable host surfaced as
  `FetchError::RobotsDenied`, because RFC 9309 makes an unfetchable robots.txt
  a complete disallow. Correct as behaviour, misleading as a crawl report — a
  user could not tell "the site forbids this" from "the site is down".
  *Fixed with T1.8.* `RobotsCache::access` now returns `Access::{Allowed,
  Disallowed, Unreadable(reason)}`, and `FetchError` gains `RobotsUnreadable
  { url, reason }`. The behaviour is unchanged — nothing on the origin is
  fetched either way — only the report can now tell them apart, which is what
  decides whether a user retries or respects the ban.
  *Note:* a 4xx on robots.txt is **not** unreadable. It means "no such file",
  which permits everything; only a 5xx or a failed connection leaves the rules
  undefined.
- [x] **T1.7** Manual redirect chain walking with loop detection and a hop cap
  *Done when:* `/redirect-chain/5` records all 5 hops; `/redirect-loop/3/0` terminates and is flagged
  *Landed as* `pounce-http::redirect` — `Fetcher::follow` returns a `RedirectChain`
  (start, hops, outcome) and never an `Err`. A loop, a hop-limit blowout, a
  malformed `Location`, and a robots-denied hop are all outcomes, because each
  one is a finding the report must show *next to the hops that led there*.
  Returning `Err` would discard the chain, which is the whole point of walking
  it by hand. 11 tests.
  *Added to `FetchConfig`:* `max_redirects`, default 10. It counts recorded
  hops, not requests sent — one request past the cap is unavoidable, since a
  response must arrive before it can be known to be a redirect. A boundary test
  asserts a chain of exactly `max_redirects` still lands, so the cap cannot
  drift off by one unnoticed.
  *Bug found by the tests:* an empty or unreadable `Location` was reported as a
  redirect **loop**. `CrawlUrl::join("")` succeeds and yields the current URL,
  so the loop check fired before the malformed-redirect check. Empty is now
  excluded explicitly.
  *Deliberately not treated as redirects:* `300` and `305` (no `Location` worth
  chasing) and `304` (a cache answer — treating it as a redirect would report
  every conditional hit as a broken chain).
- [x] **T1.8** Response metadata capture — status, timing, size, content-type, headers
  *Landed on `Fetched`.* `size()`, `declared_length` (the server's
  `Content-Length`), `content_type()` verbatim, and `mime()` / `charset()` /
  `is_html()` normalised. Keeping the declaration next to the bytes read is
  what makes truncation legible: the report can say *4 KB of a declared 8 MB*
  rather than just *4 KB*. 13 tests.
  *Timing split into two.* `time_to_headers` alongside `elapsed`, because they
  diagnose different faults — a slow `time_to_headers` is an overloaded server,
  a large gap between them is a big or slowly streamed page. One number makes a
  2 MB page indistinguishable from a struggling backend, which matters for a
  product positioned on speed.
  *No MIME crate.* The crawler asks this header two questions — what type, what
  charset — and modelling the full grammar earns nothing for them. Parsed in
  four lines; no dependency added.
  *Content sniffing deliberately omitted.* A response with no `Content-Type` is
  reported as untyped rather than guessed at. Sniffing would put a guess into a
  crawl report, which is worse than saying the server declared nothing.
  Revisit only if real sites make it necessary.

### Parsing — `pounce-parse`

- [x] **T1.9** `PageRecord` type definition (shared vocabulary — define before the extractors)
  *New crate `pounce-parse`*, depending on `pounce-http` so `PageRecord::from_fetched`
  can seed the transport facts in one call rather than twelve assignments at
  every call site. Acyclic; nothing else changes shape.
  *Two rules the type enforces.* **Raw and resolved are both kept** — a
  `canonical` of `/x` is shown to the user as written and compared as
  `https://host/x`, because a remediation message that quotes a URL appearing
  nowhere in the source is not actionable. **Absent is not empty** —
  `<title></title>` and a missing `<title>` are different findings, so
  `Option<String>` never collapses them, and the same holds for `alt`.
  *`open_graph` is a list, not a map.* Duplicate `og:` properties are
  themselves a finding; a map would silently keep the last one.
  *`MetaRobots` merges by union.* A page can carry `robots`, `googlebot`, and
  an `X-Robots-Tag` at once, and no source may loosen what another tightened.
  `noindex, all` stays noindex — reading that as indexable would be a silent,
  high-consequence misreport, and there is a test for exactly it.
  *`CrawlUrl` serde is hand-written, not derived.* Deriving would let a
  hand-edited `.pounce` file produce a `CrawlUrl` that never passed
  `from_url` — an unfetchable scheme or a missing host. Deserialisation
  re-parses. This is a trust boundary, so it is not the place to be lazy.
  11 tests, including a deliberate mutation check that the `noindex, all`
  assertion actually fails when the parser is broken.
- [x] **T1.10** Single-pass `lol_html` extraction: links, title, meta description, H1/H2, canonical, meta robots, hreflang, Open Graph, images + alt, word count
  *Done when:* golden-file tests pass over a committed corpus including malformed markup — 5 corpus files, 26 tests.
  *Word counting is streaming.* `lol_html` splits text across chunk
  boundaries, so the counter carries one bool between chunks rather than
  joining them. Constant memory for a number that fits in a `u32`; joining
  first would make peak memory proportional to page size, which is the exact
  profile this project exists to avoid.
  **Dependency added: `html-escape`** (pure Rust, no build script). `lol_html`
  hands back raw source text, so `&amp;` stayed literal in titles. A hand-rolled
  entity table is where this goes subtly wrong — real titles are full of
  `&rsquo;`, `&ndash;`, `&nbsp;` — and a title-length rule counting `&amp;` as
  five characters is a wrong audit, not a rough one. This is the one place in
  the crate where a dependency beat writing it.
  *Four bugs the tests found, all silent in production:*
  1. A second `<title>` appended to the first, reporting `FirstSecond`.
  2. Text inside `<a name="x">` (no `href`) landed on the **previous** link's
     anchor text, corrupting a link that parsed correctly.
  3. Entities were never decoded (above).
  4. The corpus itself was wrong: `<title>` is RCDATA, so tags after an
     unclosed one are *text*, not elements. The test expected something HTML
     cannot do. Corpus corrected rather than the parser.
  *Goldens are read, not accepted.* `UPDATE_GOLDEN=1` regenerates; every value
  in the five files was checked by hand against what the markup actually means.
  The golden test also asserts it checked at least 5 files, so an emptied
  corpus cannot pass by doing nothing.
- [x] **T1.11** Non-HTML handling — PDFs, images, oversized bodies, wrong content-type
  *Done when:* `/huge/8` is capped rather than buffered whole — asserted in
  `pounce-http/tests/response_metadata.rs`; the cap itself landed with T1.6.
  *`BodyKind` decides who parses.* Only `Html` reaches the extractor, so a PDF
  is recorded as *a file that never had a title* rather than *a page with no
  title* — an empty record and a non-HTML record read identically otherwise,
  and one is a finding while the other is not.
  *`Undeclared` is a separate kind from `Other`.* "The server declared nothing"
  and "the server declared something we skip" are different server-side
  problems, and merging them loses the one a user can fix.
  *Sniffing may contradict a declaration, never override it.* Magic bytes
  detect a PDF served as `text/html` and set `content_type_mismatch`; the
  declared kind still stands. Trusting the bytes instead would quietly paper
  over the server's misconfiguration, which is the actual finding — and would
  reintroduce the guessing T1.8 deliberately left out. Signatures are limited
  to ones with no realistic false positive; anything else returns "no opinion",
  because a shaky sniff manufactures findings.
  *A truncated HTML body is still extracted from.* The cap protects memory; it
  must not blank the record. What was read is a partial page and `truncated`
  already says so. 15 tests.

### Storage — `pounce-store`

- [x] **T1.12** SQLite schema + migrations; WAL; indices on every sortable column
  **Dependency chosen: `rusqlite` 0.37 with `bundled`.** Rejected `sqlx`: it is
  async, and a local file needs no runtime between the batched writer and the
  disk; its compile-time query checking also cannot see M3's dynamic `WHERE`,
  which is the only hard query problem here. `bundled` is a C build — the same
  objection that keeps `zstd` off — but the tradeoff differs: `zstd` was
  optional, SQLite is the architecture, and bundling pins one engine version
  across all three platforms instead of chasing whatever the OS ships.
  *Migrations run off SQLite's own `user_version`*, so there is no bookkeeping
  table that can disagree with reality. Each runs in its own transaction: a
  half-applied schema is worse than an unopenable file, because it looks like
  it worked. A file from a newer build is **refused**, not opened — writing
  rows a newer schema cannot read back is silent data loss.
  *`STRICT` tables.* A status stored as the text `"200"` sorts as text, so the
  grid would put 99 after 100. Rejecting the type mismatch at write time is the
  only place that error is cheap.
  *`synchronous = NORMAL` under WAL.* Risks losing the last commits on an OS
  crash, never a corrupt file; a crawl is re-runnable and resumable, so an
  fsync per batch buys durability nobody needs at whole-crawl throughput cost.
  *Repeating fields are JSON columns*, marked `ponytail:`. Headings, images,
  hreflang and Open Graph belong to the detail pane, fetched one row at a time;
  normalising them would cost a join per visible row to show what nobody sorts
  by. Upgrade path when an audit rule needs to filter inside one: a generated
  column plus an index, which SQLite adds without rewriting the JSON. 9 tests,
  including one asserting every sortable column actually uses an index in
  `EXPLAIN QUERY PLAN` rather than merely having one declared.
- [x] **T1.13** Batched writer, ~500 records per transaction
  *Done when:* a criterion bench records sustained insert throughput — **measured
  2026-08-21 on Apple M5**, `cargo bench -p pounce-bench --bench store`, full
  writeup in [`docs/benchmarks/2026-08-21-store-insert-throughput.md`](docs/benchmarks/2026-08-21-store-insert-throughput.md):

  | batch | page-only rows/s |
  |---|---|
  | 1 | 15.9k |
  | 100 | 72.5k |
  | **500** | **95.7k** |
  | 2,000 | 106.7k |

  **Historical page-only result.** ~500 captured 6× the per-row rate, and
  2,000 bought 11% more while quadrupling the rows an interrupted crawl loses.
  T1.14's link-aware re-take supersedes the earlier conclusion that the writer
  could not be the bottleneck; see that task and the benchmark writeup.
  *No pending buffer.* The transaction stays open and rows go in as they
  arrive, so peak memory is one record rather than a batch of them. The crash
  window is identical either way.
  *Upsert on `url`, not `INSERT OR REPLACE`.* Replace deletes and re-inserts,
  changing the `id` and orphaning every link edge that points at the page. A
  resumed crawl re-fetching a URL updates in place; there is a test asserting
  the id survives.
  *Dropping without `flush` rolls back.* A writer dropped mid-crawl is a crawl
  that stopped, and committing on the way out would make `committed()` a lie at
  the exact moment it is read — during resume. 9 tests.
- [x] **T1.14** Link graph tables (inlinks/outlinks) with the join indexed
  *Shape:* one `links` edge table serves both directions: `source_page_id` is
  indexed for outlinks, while `target_url` is indexed for the inlink join to
  `pages.url`. The target is a URL rather than a foreign key because an edge is
  discovered before its target is crawled, and broken or external targets may
  never receive a page row. Re-fetching a source replaces its edges inside the
  same transaction as the page upsert, so stale links cannot survive a resume.
  Mirrored inlink/outlink tables were rejected: they double the hottest write
  volume and create two copies of the graph to reconcile. 4 tests.
  *Performance re-take:* the 5,000-page fixture contains 104,807 links. At
  batch 500, writing both tables measured **4.32k pages/s / 94.9k total SQL
  rows/s** (`1.1573s`), below the fixture's ~17k req/s ceiling; storage now
  bounds an unrestricted local crawl. Batch 2,000 reached 5.28k pages/s (22%
  more), but was rejected because it quadruples the uncommitted window on a
  polite single-host crawl. Repeated batch/500 medians ranged 4.32–7.59k in
  this session, so the final full run is recorded and the variability is
  flagged rather than hidden.
- [x] **T1.15** Crawl state persistence and resume
  *Done when:* a crawl killed at 50k URLs resumes and completes with no duplicates and no losses
  *Shape:* a singleton `crawl` row stores the seed; `frontier` stores each
  normalised URL once with its shallowest discovered depth. Discovery is
  batched in one transaction; discoveries made during a crawl go through the
  existing `Writer`, so they share its transaction rather than contending for
  SQLite's single write lock. There is deliberately no duplicated completion
  flag: a frontier URL is complete exactly when a `pages` row exists, derived
  with an indexed join on `url`. This makes a killed open batch roll back the
  result and its completion together without another write on the hot path.
  *Gate exercised at full size:* a 100k-URL test attempts 50,250 page writes,
  kills the writer with exactly 50,000 committed, reopens with exactly 50,000
  pending, completes them, and asserts 100,000 distinct frontier and page rows
  with no pending URLs. The five-test state suite completed in **8.58s** in
  debug on Apple M5. 8 tests total, including schema-v2 migration, invalid
  persisted-URL rejection, and discovery/page transaction sharing.
  *Performance:* after pre-seeding the frontier in the benchmark setup, batch
  500 measured **4.45k pages/s / 97.7k inserted rows/s** (`1.1238s`), within
  the T1.14 run's noise; persistence adds no measured writer-path regression.
  *Open for T1.17:* a terminal fetch failure that produces no `PageRecord` has
  no persisted outcome shape yet and therefore remains pending after restart.
  Pipeline assembly must store that terminal outcome rather than retry it on
  every resume.
  *Deferred to T1.18:* limits and lifecycle settings have no type yet, so this
  migration persists the seed and frontier only; resume must persist the final
  settings shape when lifecycle introduces it rather than inventing JSON now.

### Orchestration — `pounce-core`

- [x] **T1.16** Frontier: priority queue + `DashMap` dedup
  *Shape:* `std::collections::BinaryHeap` provides shallow-depth-first
  priority and FIFO order within a depth; `DashMap<CrawlUrl, Seen>` is the
  global concurrent dedup set. A queued URL rediscovered at a shallower depth
  gets a new heap entry, and the superseded entry is skipped on pop rather than
  paying for arbitrary heap deletion. Popped and restored-complete URLs remain
  in `seen`, so later links cannot fetch them again. `restore` accepts the
  `(url, depth, done)` shape T1.15 loads without coupling core back to store.
  **Dependency added: `dashmap` 6.2.1** (workspace requirement `6.2`), the
  dependency named by the approved design for sharded concurrent dedup. Queue
  ordering stays in the standard library; no priority-queue crate was added.
  6 tests, including eight concurrent producers issuing 8,000 pushes for
  1,000 URLs and observing exactly 1,000 unique pops.
  *Resolved by T1.17:* failed handoffs can requeue an in-flight item; Tokio's
  bounded channels own wake-up behavior, so the frontier needs no async
  notification primitive of its own.
- [x] **T1.17** Pipeline assembly with bounded channels between every stage
  *Done when:* peak RSS stays flat while crawling 500k URLs — proving backpressure works
  *Shape:* `pounce-core::run_pipeline` connects frontier → fetch → parse →
  writer with three bounded Tokio `mpsc` channels. Fetch work is async and
  concurrency-limited; parse work uses Tokio's blocking pool; the writer stays
  single-consumer. Stage values are generic so `Result` terminal outcomes flow
  to the writer instead of being dropped, without reversing the existing
  core ← HTTP ← parse/store dependency direction. Failed frontier handoffs can
  be requeued explicitly. Schema v4 adds `crawl_failures`; a failure and its
  completion state share the writer transaction, so resume does not retry a
  terminal transport failure forever.
  **No new dependency:** core uses the workspace's existing Tokio runtime.
  7 normal tests cover all handoffs, bounded-source behavior, terminal-error
  delivery, requeue, migration, and commit/rollback semantics.
  *Measured:* `cargo test --release -p pounce-bench --test pipeline_rss --
  --ignored --nocapture` currently measures **3.44 MiB at 50k URLs and 3.80
  MiB at 500k URLs (0.36 MiB growth)** with 4 KiB response bodies and channel
  capacity 32; combined wall time was 5.28s on Apple M5/macOS 27.0. Full method and caveats:
  `docs/benchmarks/2026-08-21-pipeline-backpressure.md`.
  *Not yet verified:* this isolates pipeline retention; Gate M1 still requires
  the full HTTP + parse + SQLite 500k fixture crawl after the CLI exists.
- [x] **T1.18** Crawl lifecycle: start, pause, resume, cancel, limits (depth, count, time)
  *Shape:* `CrawlLifecycle` owns atomic running/paused/cancelled/completed/limit
  states and a Tokio `Notify`; pausing stops new pipeline admissions, resuming
  wakes them, and cancellation is terminal for that controller and wakes a
  paused crawl. Already admitted bounded work drains rather than being lost.
  Depth limits skip an item, count limits stop before admission N+1, and time
  limits measure active time so a pause does not consume the allowance.
  `CrawlLimits::allows_depth` exposes the identical check for discovery code,
  preventing future callers from durably queuing URLs they intend to exclude.
  Schema v5 stores the three settings as nullable, checked columns on `crawl`;
  NULL means unlimited and old files preserve that behavior. Runtime status is
  deliberately process-local: durable frontier outcomes already determine
  what a later explicit resume can do, while persisting `running` would invent
  crash-recovery semantics before the CLI owns reopen behavior.
  **No new dependency:** atomics, `Instant`, and the existing Tokio runtime are
  sufficient. 8 normal tests cover transitions, pause timing, cancellation
  wake-up, all three limits, controlled pipeline behavior, v4 migration, and
  settings persistence.
  *Performance recheck:* lifecycle admission is now on the existing release
  RSS probe's path, so it was rerun and later superseded by T1.19's recheck;
  see the benchmark document for the current samples. The pipeline remains
  flat. No standalone lifecycle throughput claim was made.
- [x] **T1.19** Progress reporting throttled to ~10 Hz
  *Shape:* `CrawlLifecycle` owns one atomic successful-writer count and exposes a
  `CrawlProgress` snapshot with status, admitted URLs, written URLs, active
  elapsed time, and derived URLs/sec. `report_progress` uses Tokio's existing
  100ms interval with missed ticks skipped, emitting immediately, at most
  ~10 times/sec, and once more with the terminal state. The pipeline increments
  progress only after the writer callback returns `Ok`, and now marks completion
  only after every stage and the writer drain. Transaction durability remains
  the concrete store writer's responsibility.
  **No new dependency:** core's tests enable Tokio's existing `test-util`
  feature for a deterministic clock. 2 new tests cover exact rate calculation,
  zero elapsed time, 100ms cadence, skipped bursts, and the final snapshot; an
  existing controlled-pipeline test now asserts its written progress count.
  *Deferred by ownership:* T4.4 adapts the callback to Tauri `Channel`; T4.6
  adds queue depth and status-code breakdown once concrete crawl/fetch types
  own those values. Core stays free of the app shell and does not invent them.
  *Performance recheck:* the release RSS probe now includes the per-write
  atomic increment: **3.44 MiB at 50k URLs, 3.80 MiB at 500k, +0.36 MiB**,
  5.28s combined. Memory remains flat. Atomic-counter throughput overhead was
  not measured separately, so no throughput claim is made.

### Headless entry point — `pounce-cli`

- [x] **T1.20** Minimal `pounce crawl <url>` writing to a `.pounce` file
  *Done when:* `bench-runner` can drive it via `--tool`
  *Landed as:* the unpublished `pounce-seo` crate installs the `pounce` binary.
  `crawl` accepts `--output` (default `crawl.pounce`) and `--quiet`, refuses to
  overwrite an existing file, follows same-site non-`nofollow` links, parses
  responses, and writes pages, links, failures, and discoveries through the
  existing SQLite writer. The existing bounded pipeline runs frontier batches
  of 4,096; this is the minimum adapter for a frontier that grows during its
  writer stage, and adds no scheduler or third-party dependency.
  *Test:* one end-to-end fixture crawl stores all 128 generated pages plus the
  linked sitemap exactly once, leaves no pending frontier entries, and checks
  overwrite refusal. TDD red was the deliberate crawl stub.
  *Measured smoke:* release `bench-runner --pages 128` successfully drove the
  binary: exit 0, 0.4s wall, 14 MB peak RSS, and 330 URLs/s as reported by the
  current runner. This is only a connectivity smoke, not a publishable
  throughput result; the runner still assumes 128 pages while the database
  correctly contains 129 reachable resources including `/sitemap.xml`.
  **No new third-party dependency:** all runtime crates were already workspace
  dependencies; `tempfile`, already locked and used elsewhere, is test-only.
- [x] **T1.21** Persist redirect-source completion without losing per-URL status
  *Found during T1.20 assembly:* `Fetcher::follow` retains a landing response
  plus hop summaries, while durable completion is inferred by joining the
  frontier URL to `pages.url`. A successful redirect therefore writes the
  landing URL but leaves its source pending on reopen; two sources landing on
  one URL can also overwrite each other's chain. Fix the representation before
  redirect audit rules or resume claim end-to-end correctness. Do not mark a
  redirect as a failure or copy the landing 200 status onto its source.
  *Landed as schema v6:* `crawl_redirects` is keyed by the frontier source and
  stores its first 3xx status, optional landing URL, full hop JSON (URL, status,
  raw location, resolved target), and terminal outcome. The landing page keeps
  its own 200 `pages` row. Resume now treats either row as completion, multiple
  sources can land on one page independently, and loops/failed later hops keep
  their evidence instead of collapsing to a plain failure string.
  *Crash safety:* redirect rows use the existing writer batching. An
  interruption before their commit leaves the source pending, so resume may
  refetch but cannot lose it. No new dependency. 2 new tests cover v5 migration,
  independent source statuses, and rollback; the CLI fixture test now also
  covers a landed two-hop redirect and a terminal loop.
  *Performance:* ordinary direct responses do not write this table, so the
  existing benchmark workload is unchanged and no new performance claim was
  made or re-measured.

**Gate M1 — the go/no-go:**
- [x] Full 100k-page fixture crawl completes with no lost or duplicated URLs
  *Measured 2026-08-21*, three consecutive runs, each: **100,001 pages, 100,001
  distinct URLs, 0 frontier entries left pending, 0 failures, every status 200**,
  2,799,795 link edges. Verified with `count(*)`/`count(DISTINCT url)` and a
  `frontier LEFT JOIN pages` for pending — not inferred from the runner.
  [`docs/benchmarks/2026-08-21-pounce-scale-100k-500k.md`](docs/benchmarks/2026-08-21-pounce-scale-100k-500k.md)
- [~] Benchmarked head-to-head against FreeCrawl and Screaming Frog on identical hardware
  *FreeCrawl done at 10k on 2026-08-21* (Apple M5, both tools same machine):
  [`docs/benchmarks/2026-08-21-freecrawl-head-to-head-10k.md`](docs/benchmarks/2026-08-21-freecrawl-head-to-head-10k.md).
  **Pounce 3,690 URL/s / 28 MB vs FreeCrawl 75.9 URL/s / 720 MB — ~49× faster,
  ~26× less memory**, with FreeCrawl on its *best* swept configuration and
  Pounce on its hardcoded 4-per-host default.
  *Screaming Frog: deliberately deferred to after v1.0 — decided 2026-08-21.*
  £199/yr, and its free tier caps at 500 URLs, so a head-to-head at any
  meaningful size needs a licence bought before the product it is meant to
  justify exists. **What this costs, stated plainly:** Screaming Frog is the
  incumbent our users actually switch from, so the gate's question is answered
  for the free competition and *not* for the tool that decides adoption.
  FreeCrawl is a fair stand-in for the category — unlimited URLs, 150+ checks —
  but it is not the incumbent. **No published claim may say or imply "faster
  than Screaming Frog" until this is run.**
  *Still open:* a **100k head-to-head against FreeCrawl.** The 49× was measured
  at 10k on a build that has since improved (2.71 s → 1.9 s), and the lead
  changes with scale, so the recorded table is stale in our favour and must be
  re-taken before it is published.
- [x] Peak RSS under 400 MB at 500k URLs — **257 MB after the scaling fix
  (2026-08-21); 228 MB on the build originally measured.**
  500,001 pages, 500,001 distinct, 0 pending, 0 failures, 13,999,791 links.
  Sampled every minute, resident memory sat flat at **144–174 MB** while the
  database grew 3.4 → 4.7 GB. RSS across the whole curve: 28 → 64 → 228 MB for
  a 50× increase in crawl size. **The disk-backed invariant is doing its job.**
  *Single run at 500k — no median, no spread.*
- [~] **Decision recorded in `docs/benchmarks/`.** If a competitor lands within ~20% of our throughput, stop and revisit positioning before building a GUI. The whole point of reaching this gate early is to be able to change course cheaply.
  *Recorded 2026-08-21:* FreeCrawl lands at **2%** of our throughput, so the
  "stop and revisit" condition is **not triggered**. The gate is not passed —
  100k, 500k RSS and Screaming Frog remain — but nothing here says change course.
  *The finding to carry into M4:* 28 MB vs 720 MB is a difference a user feels;
  2.7s vs 132s **on localhost** partly evaporates behind real network latency,
  which is the project's own Risk #1 and is untouched by this benchmark.

**Found while benchmarking — fix before publishing any number:**
- [x] **Throughput degraded near-quadratically with crawl size — fixed
  2026-08-21, 9.76× at 500k** (1,391.5 s → 142.6 s, 359 → 3,506 URL/s, RSS
  228 → 257 MB). Scaling 100k→500k went from 21.1× wall per 5× pages to 7.39×.
  [`docs/benchmarks/2026-08-21-scaling-fix.md`](docs/benchmarks/2026-08-21-scaling-fix.md)
  *Two structural fixes.* **Deferring `links_target`** to an end-of-crawl build
  (migration 007 + `Store::build_query_indices`) was 3.70× on its own — and
  settled the diagnosis: the `frontier`'s `WITHOUT ROWID` TEXT key is **not**
  the dominant cost, so no redesign is needed there. **Deduping before
  persisting** in the CLI took the remaining 2.64×: `discover()` was called on
  every extracted link and *then* pushed to the in-memory frontier, so ~14M
  upserts happened where ~500k were needed.
  *Both PRAGMA changes backfired and are now asserted against by tests.*
  `cache_size = 64 MB` was **slower and 99 MB heavier** at 500k (152.8 s /
  356 MB vs 142.6 s / 257 MB); at 100k it bought no measurable speed for 128 MB.
  `temp_store = MEMORY` took peak RSS to **1,592 MB and failed the 400 MB
  gate** — harmless in itself, catastrophic *because* deferring the index turns
  it into a 14M-row external sort that the pragma then holds in RAM.
  **Wall time alone rated that run the best of the day; only the RSS gate caught
  it.** A throughput-only benchmark would have shipped it.
  *Still super-linear* at 7.39× per 5×. Remaining candidates — `pages.url`'s
  unique index and the frontier's TEXT key — are unmeasured.
  *Superseded, not wrong:* the 10k FreeCrawl head-to-head recorded Pounce at
  2.71 s / 28 MB; it is now 1.9 s / 24 MB, and the gap widens with scale.
- [x] **`bench-runner` assumed each tool crawled `--pages` URLs** and derived
  URLs/s from that assumption. It reported 10,000 for both tools when the truth
  was 10,001 and 10,006. Flagged on the 2026-08-20 redo list; **fixed
  2026-08-21.**
  *`pages_crawled` is now `Option<u64>`* and `urls_per_sec()` returns `None`
  when it is unset. There is no honest fallback: guessing the numerator
  produces a number that looks like a measurement and is not one. The markdown
  table prints `?`, and a test asserts the fixture size can never leak in as a
  stand-in.
  *New `--count NAME=COMMAND`*, run after that tool finishes, whose stdout must
  be a single integer. The runner cannot know each tool's output format, so the
  operator supplies the one-liner — `sqlite3 out.pounce 'select count(*) from
  pages'`, or `jq` over a competitor's JSON summary. Any failure (spawn,
  non-zero exit, unparseable) reports unknown rather than failing the run or
  inventing a zero. A `--count` naming no `--tool` is rejected outright, since
  that typo would otherwise silently produce an unknown count in a published
  table.
  *Verified end to end:* a 500-page fixture reports **501** crawled — the count
  the old code would have got wrong. 9 runner tests, 35 report tests.
- [ ] **`pounce crawl` has no tuning flags** — concurrency is hardcoded at the
  `FetchConfig` default of 4 per host. Pounce ran handicapped in the benchmark
  above and still won, but no future benchmark can be called fair in the other
  direction until a tuning surface exists. Belongs to T6.1.
- [x] *Corrected:* FreeCrawl's `exit 1` is **not** a failure — it exits non-zero
  when any status ≥ 400 is found, which the fixture serves deliberately. The
  2026-08-20 probe recorded that exit as an unclean run; that reading was wrong.
  Its 121 failed *requests* were real and are a separate matter.

---

## M2 — Audit engine

**Goal:** 30 rules, running incrementally during the crawl.

**Design and plan:** [`docs/specs/2026-08-21-audit-rule-engine.md`](docs/specs/2026-08-21-audit-rule-engine.md)
· [`docs/plans/2026-08-21-m2-audit-engine.md`](docs/plans/2026-08-21-m2-audit-engine.md).
Classifying the 30 rules by the data each needs gives **17 per-page and 13
cross-page**, so the engine is two traits sharing one registry: `PageRule` sees
a `PageRecord` and nothing else — which puts Gate M2's 10% budget in the type
system rather than in a convention — and `SiteRule` sees the finished database.

- [x] **T2.0** *(added)* Fixture emits external links and `rel="nofollow"`
  *Prerequisite flagged under T1.2.* `broken external link` and `orphan page`
  need links that leave the site, and nothing in the fixture ever left it.
  Hosts are `.invalid` (RFC 2606) so a crawler must report them unreachable
  rather than reaching something real; one external link per page and a quarter
  of pages marking their first outlink `nofollow`.
  *Correctness confirmed:* page counts are unchanged at 10k/100k/500k and link
  counts rose by exactly one per page — the new links are recorded as edges and
  never followed, because they are off-site.
  *Baselines re-taken* (rendered bytes changed): 10k **1.6 s / 25 MB**, 100k
  **17.8 s / 75 MB**, 500k **133.9 s / 279 MB**, all single runs and all within
  variance of the previous figures. Recorded as a *current baseline* section in
  `docs/benchmarks/2026-08-21-scaling-fix.md` rather than overwriting that
  document's before/after table, which was a clean A/B on one fixture and stays
  valid only if left alone.
  *Found in passing:* `crates/pounce-bench/README.md` recorded 186 MiB/s parse
  throughput **with no host**. It was a Windows laptop figure. Replaced with the
  M5 measurement (787 MiB/s, 1.24 µs) and labelled — the 4× gap is hardware, not
  progress, and an unlabelled benchmark figure is unusable.
- [x] **T2.0b** *(added)* `PageRecord` gains `title_count` and `body_hash`
  Two rules could not fire. `multiple <title>` had nothing to read — the
  extractor keeps the first and discards the rest — and `duplicate body` needed
  comparable content without retaining it, since holding 500k bodies would undo
  the flat-memory property.
  *Hash is hand-rolled FNV-1a with known-answer tests*, for the reason
  `pounce-bench` hand-rolls its PRNG: the value is **persisted**, so it must be
  identical in every build forever, and `DefaultHasher` gives no cross-release
  guarantee. **The plan's `foobar` vector was wrong** (`0x8506…` vs the real
  `0x8594_4171_f739_67e8`); computed independently rather than copied, and one
  golden `body_hash` was re-verified against a separate implementation.
  *Deviation from the plan:* it hashed `collapse(chunk)` per chunk, which makes
  a page's hash depend on where `lol_html` splits the text — `"alpha be"` +
  `"ta gamma"` hashes as `"alpha be ta gamma"`. The hash now shares the word
  counter's boundary logic and buffers only the word being read, so memory
  stays O(one word) and the hash is over exactly the text the count counts.
  *`body_hash` is `None` for a page with no text*, not the hash of the empty
  string, or every blank page would duplicate every other.
- [ ] **T2.0c** *(discovered)* **`body_hash` and `title_count` are not
  persisted.** `pages` has no column for either, so `duplicate body` — a
  `SiteRule` that reads SQL — cannot be written. `multiple <title>` is
  unaffected, being a `PageRule` that reads the record directly. Add both
  columns in the storage task (plan Task 6) before any rule batch depends on
  them; `body_hash` is `INTEGER` and nullable, `title_count` `INTEGER NOT NULL`.
- [x] **T2.1** `Rule` trait + registry: stable id, severity, description, remediation text
  *Part one landed:* new crate `pounce-audit` with `Severity`, `RuleMeta`,
  `Issue`. The traits and registry are the next step.
  *`Severity` has no `Pass`.* The spec's fourth state is a UI rendering of
  "checked, nothing found"; a row per rule per page to record absence is 15M
  rows at 500k. A pass is the absence of an issue, not a kind of one.
  *Ordered most-urgent-first* so a plain `sort()` puts critical at the top —
  the grid sorts on this column and "critical after notice" is a wrong report.
  *`from_str` never defaults.* A corrupt file, or one from a newer build with a
  severity this one has not heard of, must not quietly become a mis-severitied
  report. One spelling serves SQL, JSON and `--fail-on`, asserted by a test,
  because three that disagree about capitalisation is a filter matching nothing.
  7 tests, mutation-checked: making `as_str` return `"Critical"` fails two of
  them, so they are exercising the code rather than agreeing with it.
  *Part two landed:* `PageRule` and the `Registry`. The trait is handed a
  `&PageRecord` and nothing else, so it **cannot** issue a query — that is what
  makes Gate M2's 10% budget (~14 s at 500k after the scaling fix) a
  compiler-checked fact rather than a convention nobody remembers at rule 23.
  *The registry owns what is true of the set*, not of any member: ids unique,
  ids matching `batch.rule-name`, total capped at 30. A rejected rule leaves
  the registry untouched, since a half-registered one would make the next
  duplicate check wrong. 7 more tests, mutation-checked twice — stopping
  `run_page` after the first rule and moving the cap by one each fail exactly
  one test, so neither is passing by accident.
  *Part three landed — `SiteRule`, sharing the registry.* Runs once against the
  finished database, returning `(url, Issue)` because a site rule *discovers*
  which pages are affected where a page rule is already looking at one. Not the
  post-pass T2.2 forbids: that prohibits a second pass over page **bodies**, and
  these are `GROUP BY`/join queries touching none.
  *One id space, one cap.* Thirty page rules plus one site rule is rejected, so
  "30 rules" stays one number rather than quietly becoming sixty. A page rule
  and a site rule sharing an id is rejected the same way two page rules are.
  *A failing site-rule query propagates* rather than being swallowed: a rule
  that could not run has found nothing, and reporting that as "no issues" is a
  clean bill of health the crawl never earned. 5 more tests (12 in the file),
  mutation-checked — counting only page rules toward the cap fails one.
  **T2.1 is complete.**
- [x] **T2.3** Issue storage and per-rule counts, queryable *(landed before T2.2)*
  *Migration 008* adds `issues`, with `severity` denormalised onto the row: the
  grid filters millions by it and a per-row join to rule metadata is the query
  pattern M3 exists to avoid. It also keeps a `.pounce` file self-contained —
  it records what was found **at crawl time**, so re-grading a rule in a later
  build cannot silently rewrite history. `ON DELETE CASCADE`, or a re-crawl
  that dropped a page would leave issues pointing at nothing and per-rule
  counts would drift upward forever. Its three indices are deferred to
  `build_query_indices`, like every other query index.
  *Migration 009 closes T2.0c* — `pages.title_count` and `pages.body_hash`.
  `body_hash` is nullable because NULL means "no text to compare", which is not
  the hash of the empty string; storing 0 would make every blank page a
  duplicate of every other.
  *`Writer::push` now returns the page id* so issues commit in the same
  transaction as their page — there is no state where a page exists with half
  its findings. The upsert preserves the row on conflict, so a resumed crawl
  re-fetching a URL gets the same id back rather than orphaning its issues.
  *`RETURNING id` was tried and dropped.* It removes a `SELECT` per page, but
  measured as a wash at 100k (medians 20.7 s vs 20.0 s, ranges overlapping), so
  it did not earn the diff.
  *Measurement, stated honestly:* three-run medians were **18.8 s without this
  task and 20.0 s with it** at 100k, ranges 18.5–19.3 and 18.4–21.7. The ranges
  overlap and **no mechanism explains a 10% cost** — two columns are ~10 bytes
  a row, `ADD COLUMN` is O(1), and `issues` is empty during these crawls. **Not
  established as a regression, and not dismissed either**; this machine's noise
  floor is wider than the effect. Re-measure under controlled conditions before
  Gate M2, which has a 10% budget riding on exactly this.
  *Test-harness fix:* every "old schema file" test hand-rolled its own undo, so
  migration 008 broke five at once. Replaced with one `rewind_to(conn, version)`
  helper; a new migration is now one line there rather than a hunt through
  tests. 008 also exposed that 005 cannot be undone with `DROP COLUMN` —
  SQLite refuses for a column named in a `CHECK` — so it rebuilds `crawl`.
- [ ] **T2.2** Incremental execution during crawl, not a post-pass
- [ ] **T2.3** Issue storage and per-rule counts, queryable
- [ ] **T2.4–T2.9** The 30 rules, in six themed batches of five, each rule with a triggering and a non-triggering fixture:
  - Response: 4xx, 5xx, redirect chains >2 hops, redirect loops, mixed-content links
  - Titles: missing, duplicate, too long, too short, multiple `<title>`
  - Descriptions: missing, duplicate, too long, too short, truncated entity
  - Headings & content: missing H1, multiple H1, empty H1, thin content, duplicate body
  - Indexability: `noindex`, canonical to non-200, canonical chain, self-referencing mismatch, blocked by robots but linked
  - Media & links: broken image, missing alt, oversized image, broken internal link, orphan page
- [ ] **T2.10** Severity assignment reviewed end to end for consistency

**Gate M2:**
- [ ] 30 rules, 60 fixtures, all passing
- [ ] Full fixture crawl produces a stable, hand-verified issue count
- [ ] Rule execution adds under 10% to crawl wall time

---

## M3 — Query layer

**Goal:** prove the load-bearing architectural claim before a single line of UI.

- [ ] **T3.1** `FilterSpec` → parameterised SQL `WHERE` (no string interpolation)
- [ ] **T3.2** `SortSpec` restricted to indexed columns, rejecting anything else
- [ ] **T3.3** Windowed `query_rows(offset, limit)` returning a `RowView` projection, not full records
- [ ] **T3.4** Aggregate queries for the issue overview
- [ ] **T3.5** Seed a 1M-row database and benchmark sort, filter, and paginate

**Gate M3 — do not proceed without this:**
- [ ] Sort of 500k rows returns in under 150ms
- [ ] Filter + sort + paginate over 1M rows stays under 300ms
- [ ] Memory flat regardless of result-set size
- [ ] Benchmarks committed to `docs/benchmarks/`

If these numbers can't be hit, the fix is indices or schema — **never** loading more into the UI.

---

## M4 — Desktop GUI

**Goal:** the app a person actually uses.

### Shell

- [ ] **T4.1** Tauri 2 scaffold, React 19 + TypeScript + Tailwind v4
- [ ] **T4.2** Design tokens from spec §3, all three theme states (light, dark, system)
- [ ] **T4.3** Tauri command layer over M3's query API
- [ ] **T4.4** Progress events via `Channel`, throttled to 10 Hz

### Crawl flow

- [ ] **T4.5** New-crawl screen: seed URL, limits, politeness settings
- [ ] **T4.6** Live progress: URLs/sec, queue depth, elapsed, status-code breakdown
- [ ] **T4.7** Pause, resume, cancel wired to the engine
- [ ] **T4.8** Open, save, and recent-crawls list

### The table

- [ ] **T4.9** TanStack Table + Virtual, server-driven rows
  *Done when:* scrolling 500k rows stays at 60fps and memory stays flat
- [ ] **T4.10** Column picker with persisted layout
- [ ] **T4.11** Sort and filter UI bound to `SortSpec`/`FilterSpec`
- [ ] **T4.12** Detail pane: full record, inlinks, outlinks, redirect chain
- [ ] **T4.13** Issue overview drilling into a filtered table

### Export

- [ ] **T4.14** CSV and JSON export, streamed from SQLite so exports never materialise in memory
- [ ] **T4.15** Export current filtered view, not just everything

**Gate M4:**
- [ ] Crawl a real site start to finish without touching a terminal
- [ ] Table responsive at 500k rows
- [ ] Cold start under 400ms
- [ ] Both themes verified

---

## M5 — MVP release (v0.1)

**Goal:** ship it.

- [ ] **T5.1** Tauri bundler config: MSI/NSIS, universal .dmg, AppImage/.deb/.rpm
- [ ] **T5.2** Code signing and notarisation — macOS notarisation is the usual multi-day surprise, start it early
- [ ] **T5.3** GitHub Actions release matrix on tag
- [ ] **T5.4** `README.md`: benchmark table above the fold, **"what this doesn't do yet"** section, dual-licence note
- [ ] **T5.5** `ARCHITECTURE.md` explaining query-don't-dump
- [ ] **T5.6** `CONTRIBUTING.md` with a dev setup someone can actually follow
- [ ] **T5.7** Publish the benchmark, including runs where competitors timed out or errored
- [ ] **T5.8** Landing page reusing the identity from `docs/product-plan.html`
- [ ] **T5.9** GitHub Sponsors; state plainly that there will never be a paid tier

**🎯 Gate M5 — MVP shipped:**
- [ ] Installers download and run clean on all three platforms
- [ ] A stranger can install, crawl, and export without asking a question
- [ ] Benchmark published and reproducible by a third party
- [ ] Zero known data-loss bugs

---

## M6 — CLI & CI (v0.2)

The clearest expression of "by developers, for developers", and the thing no GUI-first competitor does well.

- [ ] **T6.1** Full `pounce crawl` surface with `pounce.toml` config
- [ ] **T6.2** JSON report output with a stable, versioned schema
- [ ] **T6.3** `--fail-on <severity>` and meaningful exit codes
- [ ] **T6.4** Crawl summary formats: table, JSON, JUnit XML
- [ ] **T6.5** Published GitHub Action wrapping the CLI
- [ ] **T6.6** Package manager distribution: Homebrew, winget, Scoop, AUR
- [ ] **T6.7** Docs: gating a deploy on SEO regressions

**Gate M6:** a sample repo's CI fails on an introduced `noindex`, and passes once reverted.

---

## M7 — JavaScript rendering (v0.3)

- [ ] **T7.1** `chromiumoxide` CDP driver behind a `RenderStrategy` trait
- [ ] **T7.2** Chrome detection with guided install; never bundle
- [ ] **T7.3** Separate concurrency budget and timeout for rendered pages
- [ ] **T7.4** Opt-in per crawl, with the cost stated in the UI
- [ ] **T7.5** JS-rendered fixture pages added to `pounce-bench`
- [ ] **T7.6** Rendered vs raw comparison view

**Gate M7:** links only present after JS execution are discovered; HTTP-only crawls show no throughput regression.

---

## M8 — Extraction & diffing (v0.4)

- [ ] **T8.1** Custom extractors — CSS, XPath, regex — with a tester UI
- [ ] **T8.2** Extracted values as first-class sortable, filterable columns
- [ ] **T8.3** XML sitemap generation with configurable rules
- [ ] **T8.4** Crawl-to-crawl diff: added, removed, changed URLs
- [ ] **T8.5** Diff report and export

**Gate M8:** diffing two crawls of a mutated fixture reports exactly the seeded changes.

*This milestone converts Pounce from a one-off audit tool into something opened weekly — the highest-leverage post-MVP work.*

---

## M9 — Integrations & scale (v0.5)

- [ ] **T9.1** Google Search Console OAuth and join on URL
- [ ] **T9.2** GA4 join
- [ ] **T9.3** Log file ingestion — Apache, Nginx, IIS, CloudFront
- [ ] **T9.4** Link graph visualisation (canvas/WebGL, never DOM nodes per URL)
- [ ] **T9.5** Scale work: 1M → 10M URLs, partitioning, index tuning
- [ ] **T9.6** Scheduled and recurring crawls

**Gate M9:** a 10M-URL crawl completes with peak RSS under 2 GB.

---

## M10 — Rule SDK (v1.0)

The endgame: the one thing that compounds, and where a free open tool permanently beats a commercial one.

- [ ] **T10.1** Choose the extension mechanism — WASM component model vs embedded scripting — and write up the decision
- [ ] **T10.2** Stable rule ABI over `PageRecord`
- [ ] **T10.3** Rule loading, sandboxing, and resource limits
- [ ] **T10.4** Rule authoring docs and a template repository
- [ ] **T10.5** Discovery and sharing for community rules

**Gate M10:** a rule written by someone else, installed from a file, runs correctly in a crawl.

---

## Standing rules

These hold in every milestone.

- **Perf regressions are broken builds.** Criterion thresholds in CI from M0 onward.
- **Every audit rule ships with two fixtures** — one triggering, one not.
- **The UI never receives the dataset.** If a task seems to need it, the task is wrong.
- **Benchmarks publish their failures.** Timed-out and errored runs stay in the table.
- **Write each milestone's detailed plan when you reach it.** Guesses made now will be wrong by then.
- **Scope creep is the identified primary failure mode.** New feature ideas go in `docs/icebox.md`, not into the current milestone.
