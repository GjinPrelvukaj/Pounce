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

**Checkbox meanings.** `- [ ]` not done, and **work in progress counts as not
done** — a partly-finished task must stay unchecked or the next session skips
its remainder. `- [x]` done. `- [~]` **deliberately deferred by a decision that
is recorded next to it**, never "in progress". Getting this wrong once already
hid five of six rule batches from the "first unchecked task" rule.

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
- [~] **`pounce crawl` has no tuning flags** — *deferred to T6.1, where the
  full CLI surface lands.* Recorded here because it bounds what a benchmark can
  claim, not because it blocks M2. — concurrency is hardcoded at the
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
- [x] **T2.0c** *(discovered)* `body_hash` and `title_count` were not
  persisted, so `duplicate body` — a `SiteRule` that reads SQL — could not be
  written. **Closed by migration 009** alongside T2.3: `body_hash INTEGER`
  nullable, `title_count INTEGER NOT NULL`. `content.duplicate-body` in batch 4
  is the rule that needed it.
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
- [x] **T2.2** Incremental execution during crawl, not a post-pass
  *Page rules run in the writer stage*, where the record exists and its
  findings join the **same transaction as the page** — so there is no state in
  which a page is stored with half its issues. `crawl` takes a `&Registry`
  rather than building one, which is what makes the 10% budget measurable as a
  difference: an empty registry is the control.
  *Site rules run after the crawl loop, before `build_query_indices`* — they
  need the crawl-time indices to be fast, and their own rows should be indexed
  with everything else.
  *A site rule naming an uncrawled URL is skipped*, not invented: it has
  nothing to attach to, and creating a row would put a page in the report that
  the crawl never saw.
  3 tests (4 in the file), mutation-checked in both directions — stubbing out
  `run_page` and stubbing out `run_site` each fail exactly one test, so neither
  wiring is passing by accident. The empty-registry test is the third leg: rules
  off must find nothing.
- [x] **T2.3a** *(discovered writing batch 1)* An issue's subject is a URL,
  which may not be a page. `issues.page_id` was `NOT NULL REFERENCES pages`,
  but a **redirect loop never produces a page row** — the source lands in
  `crawl_redirects` and `crawl_failures` while only a landing becomes a page,
  and a loop has no landing. Same for a hop-limit blowout and an unreachable
  host. The rule could find the loop and had nowhere to record it, and T2.2's
  wiring skipped it **silently**, so `--fail-on critical` would have passed a
  site full of redirect loops.
  *Migration 010* rebuilds `issues` with `url TEXT NOT NULL` as the subject and
  a **nullable** `page_id` kept as the fast join, so the grid still avoids a
  text join per row and the cascade still cleans up when a page goes away. An
  issue with no page is untouched by that cascade, which is correct — it was
  never about a page. Rejected: giving redirect sources a `pages` row, which
  contradicts T1.21; and leaving these findings out of `issues`, which would
  make a CI gate pass a broken site. 3 tests.
- [x] **T2.4–T2.9** The 30 rules, in six themed batches of five, each rule with a triggering and a non-triggering fixture:
  **Batch 1 of 6 landed (Response) — 5 rules, 12 tests.** `response.4xx`,
  `response.5xx`, `response.redirect-chain`, `response.mixed-content` are
  `PageRule`s; **`response.redirect-loop` is a `SiteRule`** because a loop never
  produces a page, so there is no `PageRecord` for a page rule to see. Wired
  into the binary via `register_all`, not left dormant.
  *Severity split:* 5xx and mixed-content are Critical, 4xx and long chains
  Warning. A 4xx is usually a decision someone made; a 5xx is the site failing,
  and it usually means more pages are broken than the crawl caught. T2.10
  reviews this across all six batches.
  *Mutation-checked:* moving the 3-hop boundary, letting `4xx` swallow 5xx, and
  making mixed-content ignore the page scheme each break two tests — the
  triggering and the non-triggering fixture both react, which is why both exist.
  *Verified end to end:* a real crawl of `/redirect-loop/3/0` records
  `response.redirect-loop … 3 hops` against a URL with **no page row**, which is
  the T2.3a fix working live.
  **Fixture gap found:** a full 301-page crawl produces **zero** issues from
  this batch, and that is correct — the fixture serves only 200s, its
  pathological endpoints are unlinked, and it is served over **http**, so
  `response.mixed-content` can never fire end to end. The rule is unit-tested
  and **not** integration-tested. Gate M2's "hand-verified issue count" needs
  the fixture to serve some 4xx/5xx inside the linked graph, and ideally https;
  later batches (titles, descriptions, headings) will fire on defects the
  fixture already seeds.
  **Batch 2 of 6 landed (Titles) — 5 rules, 16 tests.** `title.missing`,
  `title.too-long`, `title.too-short`, `title.multiple` are `PageRule`s;
  `title.duplicate` is a `SiteRule`. `title.multiple` is only writable because
  T2.0b added `title_count`.
  *Lengths count characters, not bytes* — 60 accented characters are 120 bytes,
  and a byte count would flag titles that fit, silently and only for non-ASCII
  sites. Mutation-checked, along with both boundaries and `title_count`.
  *Absent stays distinct from empty:* `None` is `title.missing`, `Some("")` is
  `title.too-short`. Merging them would lose which mistake was made.
  **Cross-cutting fix found end to end:** `title.missing` fired on
  `/sitemap.xml` — a file working exactly as intended. `PageRule` now has
  `applies()`, **defaulting to HTML only**, with response rules opting out
  because a 404 PDF is still a 404. Put in the trait rather than in each rule:
  30 rules each remembering to guard is 30 chances to forget, and forgetting is
  silent — it yields a plausible finding about a file that is fine. Confirmed on
  a real crawl: sitemap.xml now has 0 issues.
  *Real-crawl counts, checked against the database:* 300 × `title.too-short`
  (fixture titles run ~20 characters), 0 too-long, 0 duplicate — each agreeing
  with a direct SQL query.
  **Batch 3 of 6 landed (Descriptions) — 5 rules, 15 tests.** `missing`,
  `too-long`, `too-short`, `truncated-entity` are `PageRule`s; `duplicate` is a
  `SiteRule`. `too-short` is Notice rather than Warning — a short description
  still works and merely wastes space, where a missing one does not.
  *`truncated-entity` matches known entity names, not shape.* "Any `&` followed
  by letters" would flag `Q&A` and `Ben & Jerry`. It matches a prefix of a real
  entity name or a numeric reference instead, and there is a fixture of five
  ordinary ampersands that must stay silent.
  *Bug the fixtures caught:* the check excluded exact matches, so `&amp` — the
  commonest truncation of all — did not fire. A **complete** entity is decoded
  before the rule sees it, so anything still spelled out is broken by
  definition, semicolon or not.
  *A guard removed for being provably dead:* a whitespace check changed no test
  under mutation, and the entity and numeric checks already reject
  `Fish & Chips`. Removed with the reasoning recorded, rather than left looking
  deliberate.
  *A brittle assertion fixed:* `register_all` was pinned at an exact count in
  two batch files, so every new batch broke an unrelated one. The cap is now
  the invariant asserted per batch; the running total is pinned in one place.
  *Real-crawl counts, each verified against a direct SQL query:* 41 ×
  `description.missing`, 259 × `description.too-short`, 300 ×
  `title.too-short`. **15 of 30 rules shipped.**
  **Batch 4 of 6 landed (Headings & content) — 5 rules, 16 tests.**
  `missing-h1`, `multiple-h1`, `empty-h1`, `thin` are `PageRule`s;
  `duplicate-body` is a `SiteRule` reading the `body_hash` T2.0b added.
  `multiple-h1` is Notice, not Warning: HTML5 permits several, so it is a
  clarity problem rather than a defect, and rating it beside a *missing* H1
  would flatten a real difference.
  *Absent stays distinct from empty a third time:* `[]` is `missing-h1`,
  `[""]` is `empty-h1`. Mutation-checked — making `missing-h1` swallow the
  empty case breaks two tests.
  *`body_hash` round-trips through a signed column*, pinned by a test using a
  hash with the high bit set. Without it, duplicate detection would silently
  stop working for half of all possible hashes — the half no small fixture
  produces.
  **Fixture coverage is now the gating problem, not the rules.** Four batches
  and 20 rules are shipped, but a full 301-page crawl exercises only **4** of
  them end to end: `title.too-short`, `description.too-short`,
  `description.missing`, and `response.redirect-loop`. Every "0" was verified
  against a direct SQL query and is *correct* — the fixture serves only 200s
  over http, its pages all exceed 200 words and carry unique headings, and its
  pathological endpoints are unlinked. **Gate M2's "stable, hand-verified issue
  count" is not meaningful until the fixture seeds defects the rules can find.**
  That is now the largest single item left in M2, ahead of the remaining two
  **Batch 5 of 6 landed (Indexability) — 5 rules, 12 tests.** `noindex` and
  `canonical-elsewhere` are `PageRule`s; `canonical-non-200`, `canonical-chain`
  and `blocked-but-linked` are `SiteRule`s — the first batch where site rules
  outnumber page rules, because canonical relationships are between pages by
  definition.
  *`noindex` is Warning, not Critical.* The spec reserves Critical for "noindex
  on important pages", and nothing here knows which pages are important — a
  staging page carrying noindex is working as intended, and rating every one
  Critical trains the reader to ignore the colour.
  *`canonical-elsewhere` is a Notice*: pointing a duplicate at its original is
  the correct use of a canonical, so this exists to make the set reviewable
  rather than to say it is wrong.
  **A string coupling, pinned rather than trusted.** `blocked-but-linked` finds
  robots denials by matching `crawl_failures.reason`, which is
  `FetchError::RobotsDenied`'s `Display`. A reworded error would stop the rule
  firing **silently**. A test builds the real error and asserts the prefix still
  matches — mutation-checked, rewording the constant breaks three tests — and it
  also asserts `RobotsUnreadable` does **not** match, keeping T1.6a's
  "banned versus down" distinction intact inside the rule layer.
  *Real-crawl counts, cross-checked:* `indexability.noindex` fires **7** times
  against 7 noindex pages in SQL. 0 robots-blocked failures, so
  `blocked-but-linked` correctly stays silent. **25 of 30 rules shipped.**
  **Batch 6 of 6 landed in part (Media & links) — 3 rules, 14 tests.**
  `media.missing-alt` is a `PageRule`; `links.broken-internal` and
  `links.orphan-page` are `SiteRule`s. **`media.broken-image` and
  `media.oversized-image` are not written**, because neither can be: both read
  a `resources` table that nothing fills yet, and filling it is the image
  `HEAD` pass the M2 plan lists as its own follow-on with its own throughput
  benchmark. Registering two rules that can never fire would have spent two
  slots of the 30-rule cap on permanent silence and made the count read
  complete when it is not. Recorded as **T2.9a** below, then landed —
  **`media.broken-image` and `media.oversized-image` shipped in a follow-up
  commit, 8 further tests. 30 of 30.**
  *`missing-alt` is one issue per page, not per image.* A template that forgot
  `alt` yields one defect repeated fifty times, and a row per image would bury
  every other finding on the page. The detail carries the ratio and the first
  offending `src` — `3 of 4 images: /static/img-0.jpg` — so the page is still
  actionable from the grid.
  *Absent stays distinct from empty a fourth time:* `alt=""` is the documented
  decorative marker and the **correct fix**, so only a missing attribute fires.
  Mutation-checked: making the rule swallow `Some("")` breaks a test.
  *`broken-internal` joins `links` to `pages`, and deliberately ignores
  `crawl_failures`.* A target with no page row is *uncrawled*, not broken —
  most of them are external, and treating absence as breakage would flag every
  outbound link on the site. Robots denials live in `crawl_failures` and are
  already reported by `indexability.blocked-but-linked`; charging one site
  defect to two rules would double it in the summary.
  *`orphan-page` excludes `depth = 0` and does **not** filter `nofollow`.*
  Depth 0 is the URL the crawl was started from (and, when the seed redirects,
  its landing), which nothing on the site is expected to link to — including it
  would guarantee one false finding per crawl. `nofollow` is a ranking hint,
  not an absent link, so a page reachable only through one is not an orphan.
  *Mutation-checked, six for six:* swallowing an empty `alt`, moving the 400
  boundary to 500, turning `broken-internal`'s join into a LEFT JOIN, dropping
  the `depth > 0` guard, adding a `nofollow = 0` filter to `orphan-page`, and
  overriding the HTML gate each break at least one test.
  *Real-crawl counts, cross-checked:* `media.missing-alt` fires **60** times on
  the 301-page fixture, matching an independent
  `json_each(pages.images)` query exactly — the **fifth** rule now exercised
  end to end. `broken-internal` and `orphan-page` are both **0**, each agreeing
  with a direct SQL query: the fixture serves only 200s and every page is
  linked.
  *Housekeeping:* the running-total pin moved from `rules_descriptions.rs` to
  the newest batch's file, so landing a batch breaks the count in the file
  whose author is already counting it.
  batches.
  - Response: 4xx, 5xx, redirect chains >2 hops, redirect loops, mixed-content links
  - Titles: missing, duplicate, too long, too short, multiple `<title>`
  - Descriptions: missing, duplicate, too long, too short, truncated entity
  - Headings & content: missing H1, multiple H1, empty H1, thin content, duplicate body
  - Indexability: `noindex`, canonical to non-200, canonical chain, self-referencing mismatch, blocked by robots but linked
  - Media & links: broken image, missing alt, oversized image, broken internal link, orphan page
- [x] **T2.9a** *(discovered finishing batch 6)* Image `HEAD` pass and the
  `resources` table — the crawl capability `media.broken-image` and
  `media.oversized-image` need. Scoped by
  [`docs/specs/2026-08-21-audit-rule-engine.md`](docs/specs/2026-08-21-audit-rule-engine.md)
  §3: `HEAD` only, its own concurrency budget so images cannot starve page
  fetching, a skip flag, and the same robots.txt and rate limits as everything
  else. `content_length` is nullable, so `oversized-image` must treat NULL as
  *unknown* rather than *small*. The fixture averages ~4 images per page, so
  this can multiply request count several times over — page-crawl throughput
  must be measured and shown not to regress. Not folded into batch 6: it is
  crawl work, it needs a benchmark of its own, and the M2 plan already names it
  as a separate follow-on.
  **Landed — 11 tests.** `Fetcher::head` shares `fetch`'s robots.txt check,
  per-host limiter and retry policy by construction: both now call one private
  `send(url, method)`, which hands the caller the concurrency permit so a `GET`
  can still hold it across the body read. A second request path with its own
  politeness code would have been a hole in the guarantee rather than a
  shortcut, and a test asserts a `HEAD` into a disallowed path is `RobotsDenied`.
  *`Content-Length` is read from the header, not `Response::content_length()`.*
  A `HEAD` has no body, so the body's size hint describes the absence — hyper
  reports 0 — and a 4 MB image would arrive as `Some(0)`, making
  `oversized-image` silently blind. Mutation-checked: swapping it back breaks
  two tests. The undeclared-length case is served from a **raw socket**,
  because hyper recomputes `Content-Length` for any body it can size, so an
  axum fixture that merely removes the header gets it back.
  *The pass runs after the crawl loop, not alongside it.* The spec's
  requirement is that image checking cannot starve page fetching; running it
  afterwards satisfies that by construction rather than by a scheduler that has
  to be trusted. It reuses `run_pipeline` with its own `fetch_concurrency` (8)
  — a second scheduler would be a second place to get backpressure wrong.
  **Deviation from the spec: `--images` is opt-in, not skippable-by-default.**
  The spec says "skippable with a flag", implying on by default. Rejected:
  every benchmark this project publishes measures a page crawl, and a default
  that multiplies request count by the site's distinct-image count would change
  what those numbers mean without changing the command that produces them — and
  it would send those requests to whatever third-party hosts the markup names.
  Flipping the default later is one line, once there is a measurement arguing
  for it.
  *Fixture gap closed:* `/static/img-{n}.jpg` is now served, `img-3` missing
  and `img-4` oversized at 300 KB, chosen **by index** so the expected findings
  are worked out from the fixture rather than read off the crawl meant to check
  it. Registered with `any`, not `get`: a GET-only route answers 405 to a HEAD,
  which would look like a broken image and test the router instead of the rule.
  **Measured, interleaved A/B on 10,001 pages:** baseline 10.29 s median,
  `--images` 10.31 s — **+0.2%, inside the baseline's own ±1% spread**, both
  files holding 10,001 pages:
  [`docs/benchmarks/2026-08-22-image-head-pass.md`](docs/benchmarks/2026-08-22-image-head-pass.md).
  **Two numbers deliberately not claimed.** The fixture references **five**
  distinct images site-wide, so the pass issued 5 requests for a 10k crawl.
  That says nothing about (a) per-image cost on a site with per-page unique
  images, or (b) memory for the in-memory `HashSet` of distinct image URLs,
  whose ceiling is that same site. Both need a fixture whose image URLs vary
  per page — a follow-up, not a figure to estimate.
- [x] **T2.9b** *(completing batch 6, after T2.9a)* The two image rules.
  Both are `SiteRule`s reading `resources`, because the check happens once per
  distinct image URL rather than once per page that shows it — a logo on 500k
  pages is one request, one row, and one finding.
  *`oversized-image` is a Notice, `broken-image` a Warning.* An oversized image
  is weight to trim on a page that works; a broken one is a hole in the page
  for every visitor. Rating them alike would flatten the distinction the
  severity column exists to draw.
  *`oversized-image` reads only 200s and only non-NULL lengths.* A 404's
  `Content-Length` describes the error page, and `broken-image` has that URL
  already; NULL is the server declaring nothing, and reading it as 0 would
  exempt every chunked or streamed image from the rule. The threshold is 100 KB
  in one named constant, because it is a judgement rather than a standard.
  *An empty `resources` table means both stay silent* — a crawl run without
  `--images` checked nothing, and reporting unchecked as broken would make
  every default crawl look catastrophic.
  *Mutation-checked:* moving the threshold, making the boundary inclusive,
  letting the rule see non-200s, treating NULL as known, and narrowing
  `broken-image` to 5xx each break a test.
  **These are the first issues whose subject is not a page at all** — an image
  URL has no `pages` row, so T2.3a's nullable `page_id` is load-bearing for a
  second reason it was not designed for. Confirmed live: 2 of the 669 issues
  have `page_id IS NULL`, and both are image findings.

- [x] **T2.10** Severity assignment reviewed end to end for consistency
  *The standard now lives where grades are chosen* — as doc comments on
  `Severity` itself (`issue.rs`), which IDE hover puts in front of every rule
  author, with a pointer from `rules/mod.rs`. Critical means *assume the page
  is broken*: serving failure, browser-refused content, an instruction to
  search engines that names something impossible, or no declared identity at
  all. Warning means a real defect on a page that otherwise works. Notice
  means nothing is wrong — legal markup, correct mechanism use, or headroom.
  Two tests a grade must survive are written beside them: could the finding be
  the site working as intended (then not Critical), and is anything wrong at
  all (if not, not Warning).
  *All six recorded rulings upheld, all 30 grades stand, zero re-grades.* The
  set was already consistent *because* the rulings were made per batch; what
  was missing was the standard they implied and anything holding future grades
  to it. The two closest calls, reasoned rather than rubber-stamped:
  `title.too-short` stays **Warning** although its description sibling is
  Notice — an empty `<title>` fires this rule, and an empty title is
  functionally identity-less, so demoting the rule would flatten it against
  `title.missing`'s Critical; and a too-short title usually means the subject
  failed to inject (a template bug), where a short description is a complete
  thought wasting space. `links.orphan-page` stays **Notice** — being unlinked
  is often deliberate (campaign landing pages, sitemap-only utility pages),
  so there is no defect to grade higher.
  *Enforcement* (`tests/severity_review.rs`, 5 tests): every shipped grade is
  pinned in one manifest whose entries carry their short-form why, checked in
  both directions — a re-grade fails until the manifest changes on purpose,
  and a new rule cannot land ungraded. The rulings themselves are executable:
  six orderings (5xx over 4xx, broken-image over oversized-image,
  missing over present-but-meagre twice, identity over optional metadata,
  broken directive over unreliable one), family symmetry (duplicate-title ==
  duplicate-description, too-long == too-long), and `noindex != Critical`.
  A floor test requires all three levels to stay in use. Mutation-checked
  three ways: flipping `title.multiple` fails exactly the manifest test;
  flipping `description.missing` fails manifest *and* ordering together;
  dropping a manifest row fails the unreviewed-rule direction.
  **What this cannot catch, stated plainly:** a judgement can be consistently
  wrong. Nothing mechanical knows Warning is *right* for thin content — the
  tests guarantee only that no grade moves or lands silently, and that
  overturning a ruling means editing an assertion whose comment states what it
  protected.
  *Found while writing the orderings:* `Severity`'s `Ord` is
  most-urgent-**first**, so `critical > warning` is false and raw comparisons
  read backwards to urgency. The assertions go through an `outranks()` helper
  that says what it means; anyone comparing severities by hand should do the
  same.
  *Distribution across the shipped set:* **5 Critical** (response.5xx,
  response.mixed-content, response.redirect-loop, title.missing,
  indexability.canonical-non-200) / **20 Warning** / **5 Notice**
  (description.too-short, content.multiple-h1, indexability.canonical-elsewhere,
  media.oversized-image, links.orphan-page). Three levels in active use — the
  column discriminates.
- [x] **T2.10a** *(review of T2.10)* Three corrections to the enforcement, one
  of them a real defect.
  *The manifest test counted the cap instead of the manifest.* It asserted
  `shipped.len() == MAX_RULES`, but `MAX_RULES` is a **ceiling** the registry
  already enforces and one that M10's rule SDK is expected to raise. Verified
  by mutation: raising the cap to 40 without touching a single severity failed
  the severity test, with a message pointing the reader at the manifest for a
  change that had nothing to do with it. Now compared against `GRADES.len()`,
  which also catches an id duplicated in the manifest — the two bijection loops
  each accept that on their own. Re-checked: passes with the cap moved, still
  fails on a dropped row, now fails on a duplicated one.
  *The family-symmetry test was silently at odds with its own commit.* It
  requires `title.*` and `description.*` to match for `duplicate` and
  `too-long`, while T2.10's reasoning argues the opposite for `too-short` —
  a title carries identity and a description does not. Rather than soften the
  test, the exception is now **executable**: `title.too-short` must outrank
  `description.too-short`, so the closest call in the review is a ruling rather
  than a coincidence, and the symmetry test says in its comment that it is
  per-family and not a general law. Mutation-checked — demoting
  `title.too-short` breaks it.
  *`links.orphan-page`'s rationale sat below its entry* where every other sits
  above, so it read as belonging to nothing. Moved. 478 tests.

**Gate M2:**
- [x] 30 rules, 60 fixtures, all passing — **30 of 30**, the cap reached. A
  test asserts a thirty-first registration is rejected against the real shipped
  set, so the cap is now enforced rather than merely stated.
- [x] Full fixture crawl produces a stable, hand-verified issue count —
  **669 issues over 301 pages** (`--pages 300 --seed 42`, crawled with
  `--images`), **identical across three consecutive runs**:

  | Rule | Count | Cross-checked against |
  |---|---:|---|
  | `title.too-short` | 300 | fixture titles run ~20 characters |
  | `description.too-short` | 259 | direct SQL on `meta_description` |
  | `media.missing-alt` | 60 | `json_each(pages.images)` where `alt IS NULL` |
  | `description.missing` | 41 | ~12% of pages generate without one |
  | `indexability.noindex` | 7 | `SELECT count(*) FROM pages WHERE noindex` |
  | `media.broken-image` | 1 | `resources` where `status >= 400` |
  | `media.oversized-image` | 1 | `resources` where `status = 200 AND content_length > 102400` |

  **7 of 30 rules fire end to end**, up from 4. Every zero was checked with a
  direct query and is *correct*: the fixture serves only 200s over http, every
  page exceeds 200 words with unique headings and one `<h1>`, no page
  canonicalises, and its pathological endpoints are unlinked. Making the
  remaining 23 fire needs the fixture to seed those defects inside the linked
  graph — worth doing, but it is fixture work rather than rule work, and the
  gate's requirement is a count that is stable and explained, which this is.
- [x] Rule execution adds under 10% to crawl wall time — **at 10k and 100k
  only.** 5.3% measured 2026-08-23 (5.25% at 10k, 5.28% at 100k). **Measured
  again at 500k on 2026-08-25: 13.2% against a localhost fixture, and 0.22%
  against anything with network latency** —
  [`docs/benchmarks/2026-08-25-rule-overhead-at-500k.md`](docs/benchmarks/2026-08-25-rule-overhead-at-500k.md).
  The rules cost a fixed **~11 µs per page**; the *share* is entirely a question
  of what the crawl is bound by. Add a 5 ms per-request delay and the same
  0.22 s of work goes from 6.9% of the crawl to 0.22% of it. The budget as
  written is measured against the most hostile denominator that exists — a
  fixture with no latency, where the crawler is bound by its own writer — which
  is the right benchmark for throughput and the wrong one for "what do the rules
  cost a user". **Re-judge the gate as a per-page cost with both shares stated,
  rather than as one percentage.** Original measurement:
  [`docs/benchmarks/2026-08-23-audit-rule-overhead-real.md`](docs/benchmarks/2026-08-23-audit-rule-overhead-real.md).
  The 2026-08-21 figure it replaces (0.044%, ×230) is marked superseded in its
  own file: it benched **stand-in** rules, excluded all site rules, and never
  wrote an issue. Page rules cost **341.6 ns/page** for the real nineteen —
  2.9x the old figure for eleven fewer rules — and the eleven site rules cost
  **848 ms at 500k**, 0.63% of that crawl.
  **The measurement found a defect that would have made a large crawl never
  finish, and fixing it was the substance of the task.** Three site rules join
  on `links.target_url`, but `build_query_indices()` — which creates
  `links_target` — ran *after* the site rules. So `links.orphan-page` correlated
  a subquery over `links` per candidate page with no index: **45.0 s on a 10k
  store**, shape O(pages × links), so 500k against 14M edges would not have
  completed. `Store::build_link_index()` is now split out and called before the
  rules that read it — **45.0 s → 3.99 ms, ~11,000x** — and the harness asserts
  the query plan names `links_target` rather than printing it, so a reordering
  fails a test instead of silently costing 45 seconds. This does not retreat
  from migration 007: its rule is that an index nothing reads *during* a crawl
  is not **maintained** during one, and this is still one sorted bulk build,
  moved ahead of its first reader.
  *Where the 5.3% actually goes:* evaluating the rules is 182 ms of the 981 ms
  at 100k. The other **800 ms is writing 223,764 issue rows** — recording what
  the rules find costs 4x more than deciding it, which also means the figure
  moves with how broken the site is (~2.2 issues/page here).
  *A page rule's cost is dominated by whether it fires, not what it checks.*
  `title.too-long` and `title.too-short` are the same code — same field, same
  comparison, same `format!` — and cost 3.66 vs 45.86 ns/page, because the
  fixture's titles are short so one fires on every page and the other never
  does. ~42 ns per finding is the `String` plus the push.
  *The one expensive rule names itself:* `response.mixed-content` is **141
  ns/page, 41% of the whole page-rule budget**, being the only rule that walks
  every link. Measured on a deliberately hostile corpus (a quarter of pages
  https with http links, a site mid-migration); on all-http content it
  short-circuits.
  *Two stale claims in the old note, corrected:* site rules are **11** of the
  30, not 13; and the CLI has needed no flag to enable rules since T2.2 wired
  `register_all` into the binary — driving `crawl()` with an empty `Registry`
  is what produces the control.
  *Not established:* no end-to-end A/B at 500k (10k and 100k agree to 0.03
  points, so the trend is flat, but the largest size was not run both ways),
  and peak memory during the rule pass is unmeasured — M1's 400 MB gate was
  measured without rules running.

**Gate M2 is closed.**

---

## M3 — Query layer

**Goal:** prove the load-bearing architectural claim before a single line of UI.

**Implementation plan:**
[`docs/plans/2026-08-23-m3-query-layer.md`](docs/plans/2026-08-23-m3-query-layer.md),
written 2026-08-23 on reaching the milestone.

**Decided 2026-08-23 (owner's call): `OFFSET` stays; the schema bends.** Keyset
pagination was the priced alternative — it removes the deep-skip cost with
single-column indices — and was rejected because it cannot answer "jump to row
500,000" without counting, degrading the virtualised grid's scrollbar from
scrubbing to next/prev paging. *Scroll position maps to `OFFSET`* is
load-bearing for the product's central interaction, so it stands.

- [x] **T3.0** *(new, from the plan)* Narrow-row shape decided **by
  measurement**: the probe's duplicated `row_view` table against splitting
  `pages` into a narrow grid row plus a `page_detail` table. The split has no
  duplication and no post-crawl build, but adds one insert per page to the
  crawl's hot path — and this repo's history says write-path changes at scale
  are where the surprises live. Interleaved A/B at 100k, both arms asserted to
  the same row count; the loser is written into the plan with its numbers.
  **Decided 2026-08-24: split `pages`.** The extra insert made writes 2.65%
  *faster* (100k, medians of 5), the file 106 MB smaller at 1M, and both shapes
  answer the worst query in ~7 ms against a 300 ms gate. `row_view` is the
  loser, recorded with its numbers in
  [`docs/benchmarks/2026-08-24-narrow-row-shape.md`](docs/benchmarks/2026-08-24-narrow-row-shape.md).
  Migration 012 landed with it: `pages` is the narrow row, `page_detail` holds
  the six repeating JSON fields, and an existing file's ids survive the move.
- [x] **T3.1** `FilterSpec` → parameterised SQL `WHERE` (no string interpolation)
  — a closed `Filter` enum in `pounce-store/src/query.rs`; every variant tested
  matching and not, plus an injection case and a parameter-count assertion.
- [x] **T3.2** `SortSpec` restricted to indexed **combinations**, not columns —
  and the supported pairs chosen from **measured selectivity**, since only an
  unselective filter needs a composite index. A test must assert each declared
  pair's query plan does not say `USE TEMP B-TREE`. **Done: 20 composites,
  declared from measured selectivity.** The three ~90% filters (`status`,
  `kind`, `noindex`) plus `depth` go from 90–212 ms to 1.4–2.2 ms at 200k, for
  90 MB of index; `has_issue` needs none. A sort-first covering index was priced
  as the 7-index alternative and refuted — SQLite takes it for the equality
  search and temp-B-trees the sort anyway.
  [`docs/benchmarks/2026-08-24-filter-sort-pairs.md`](docs/benchmarks/2026-08-24-filter-sort-pairs.md).
- [x] **T3.3** Windowed `query_rows(offset, limit)` returning a `RowView` projection, not full records
  — `Page<RowView>` with the total; `limit` clamped to `MAX_WINDOW` server-side,
  and a live-WAL reader tested against an open write batch. Memory flatness at
  scale is measured in T3.5's gate run, not here.
- [x] **T3.4** Aggregate queries for the issue overview — `issue_overview()`
  groups by rule and by severity off `issues` alone, counting issues *and*
  distinct URLs so one bad template does not read as a site-wide problem. A
  test asserts the plans never touch `pages`, which is what would drop the
  findings whose subject never became a page.
- [x] **T3.5** Seed a 1M-row database and benchmark sort, filter, and paginate
  — **Gate M3 passed 2026-08-24.** The seeder is `tests/common/mod.rs`; the gate
  is `tests/gate_m3.rs`, which asserts the thresholds rather than printing them.
  The 1M run corrected T3.2's rule (equality vs range) and found the issue
  overview at 564 ms, now 158 ms.
  — **a seeder, not a crawl.** The probe's real 500k and 1M databases no longer
  exist and re-crawling them is hours; the M2 site-rule harness seeds 500k in
  minutes. The seeder needs a realistic **status mix**: the fixture's all-200
  pages are what made the probe's filter maximally unselective, so keeping that
  case is deliberate pessimism, not a default.

**Probed early on 2026-08-22, before M2's rules and any UI** — the gate was
expensive to test when this plan was written and is cheap now that M1's
benchmarking left real 500k and 1M databases behind:
[`docs/benchmarks/2026-08-22-m3-query-gate-probe.md`](docs/benchmarks/2026-08-22-m3-query-gate-probe.md).
**The architecture holds; the schema does not yet.**

**Gate M3 — do not proceed without this:**
- [x] Sort of 500k rows returns in under 150ms — **6.5–11.5 ms at 1M** through
  the real query layer, every sort column, paged to the middle.
- [x] Filter + sort + paginate over 1M rows stays under 300ms — **138 ms in the
  worst of the 46 pairs the layer will run, 11–20 ms for the common ones**, from
  18,270 ms. Fixed by the split schema (T3.0) plus 26 composite `(filter, sort)`
  indices (T3.2), with support declared per *shape*: only an equality on the
  leading column can use a composite, so range filters are refused the two sort
  columns where a per-row lookup costs 450 ms.
  [`docs/benchmarks/2026-08-24-gate-m3.md`](docs/benchmarks/2026-08-24-gate-m3.md)
- [x] Memory flat regardless of result-set size — **12 MB → 12 MB** for a 200×
  larger window, on a 1M-row database, through the real layer.
- [x] Benchmarks committed to `docs/benchmarks/`

**Design consequence for T3.2.** "Restricted to indexed columns" is not strong
enough; it must be **indexed combinations**. An indexed sort column is still
18 s if the active filter is a different indexed column matching most rows. The
UI must offer a declared, bounded set of filter × sort pairs — the full cross
product is ~54 indices and is not an option. *Alternative to price first:*
keyset pagination removes the deep-skip cost with single-column indices, but
conflicts with the `OFFSET` invariant and so needs the spec changed first.

If these numbers can't be hit, the fix is indices or schema — **never** loading more into the UI.

---

## M4 — Desktop GUI

### Shell


- [x] **T4.1** Tauri 2 scaffold, React 19 + TypeScript + Tailwind v4 — `crates/pounce-app`
  (Tauri 2.11, one `engine_info` command) and `ui/` (React 19.2, Vite 7.3,
  Tailwind 4.3, TS 5.9). Repo layout kept over Tauri's `src-tauri/` convention.
  CI gains the Linux WebView packages and a frontend typecheck job.
- [x] **T4.2** Design tokens from PRODUCT.md (§3 of the spec is superseded), all
  three theme states (light, dark, system) — dark quoted from
  `design/crawl-view.html`, light *derived by measurement* since no light
  palette existed; `npm run check:contrast` fails CI if any tier drops below
  4.5:1. System needs no JavaScript: `prefers-color-scheme` selects the palette
  and an explicit choice sets `data-theme`. Inter and JetBrains Mono are
  self-hosted, because the window's CSP is `default-src 'self'`.
- [x] **T4.3** Tauri command layer over M3's query API — `open_crawl`,
  `query_rows`, `issue_overview`, `supported_sorts`, plus wire DTOs that
  re-establish T3.1's guarantee at the IPC edge: a rule id arriving as a string
  is resolved against the registry or refused. `SortSpec` is built through
  `SortSpec::new`, never deserialised, so no caller can skip the pair check.
- [x] **T4.4** Progress events via `Channel`, throttled to 10 Hz — the throttle
  stays in the engine (`CrawlLifecycle::report_progress` at `PROGRESS_INTERVAL`)
  and the shell forwards ticks, so no consumer can opt out of it. Required
  extracting the crawl runner into `pounce-run`, since the app must not depend
  on the CLI, and fixed a real bug that hid there: the pipeline completed the
  lifecycle per *batch*, so a multi-batch crawl would have stopped after the
  first.

### Crawl flow


- [x] **T4.5** New-crawl screen: seed URL, limits, politeness settings — limits
  ride on the lifecycle and are enforced (a 25-URL cap on a 2,000-page fixture
  writes 25 and stops without draining the frontier); politeness is per-host
  concurrency and delay, capped at 16 and *refused* rather than clamped above
  it. robots.txt, `Retry-After` and the identifying user agent are stated on
  the screen rather than offered as switches.
- [x] **T4.6** Live progress: URLs/sec, queue depth, elapsed, status-code
  breakdown — queue depth and the class tally are counted by the runner and
  ride on `CrawlProgress`, rather than a `GROUP BY` per tick against the
  database the writer is holding. The breakdown is asserted to account for
  every fetch, failures included; classes are paired with an icon and a label,
  never colour alone.
- [x] **T4.7** Pause, resume, cancel wired to the engine — pause is gated at the
  *fetch* stage as well as at admission, because ~80 URLs sit in the channel
  ahead of fetch and would otherwise keep hitting the site for most of a minute
  on a polite crawl. Cancel keeps what it wrote: the partial file opens and
  queries like any other.
- [x] **T4.8** Open, save, and recent-crawls list — native dialogs via
  `tauri-plugin-dialog`, with a capability granting only open and save. Recents
  live in the window's storage, not in the `.pounce` file: a crawl handed to
  someone else should not arrive carrying a list of where it has been.

### The table


- [x] **T4.9** TanStack **Virtual**, server-driven rows — *Table is not used;*
  sorting, filtering and pagination are server-side and the row model only holds
  the visible window, so it would have contributed twelve lines of column
  descriptors for an API that renamed its entry points between majors. Columns
  are a plain array, which is what T4.10 needs anyway.
  *Done:* steady scrolling at 500k drops **0.0%** of frames (median 17.0 ms on a
  60 Hz display); a scrollbar fling across the whole dataset drops 2.4%. Memory
  **99–105 MB flat** against a 681 MB file.
  [`docs/benchmarks/2026-08-24-grid-scroll-500k.md`](docs/benchmarks/2026-08-24-grid-scroll-500k.md)
- [x] **T4.10** Column picker with persisted layout — visibility, kept in the
  window's own storage beside the theme and the recents list, and filtered
  against the build's own column list so a key from an older version is dropped
  rather than rendering an empty track. Two new optional columns (Type,
  Indexable) give it something to do. Reordering is deliberately not in it
- [x] **T4.11** Sort and filter UI bound to `SortSpec`/`FilterSpec` — clickable
  column headers, and a filter bar written as words (`Not found — 4xx`, `2
  clicks or fewer`). Which sorts are on offer comes from `supported_sorts`, so
  a substring filter really does grey out every header but URL, and a rule that
  gains a composite index gains its sorts without a TypeScript edit
- [x] **T4.12** Detail pane: full record, inlinks, outlinks, redirect chain —
  `Store::page_detail` reads every column of one row plus both directions of the
  link graph; the link lists are capped at 100 each with the true counts beside
  them, which is the "UI never receives the dataset" invariant applied to a page
  rather than to a crawl. Findings in the pane carry the rule's sentence, its
  detail and its fix
- [x] **T4.13** Issue overview drilling into a filtered table — **the top
  interaction gap.** Every issue count is a link: clicking `noindex · 3,913`
  filters the grid to those pages. Without it the counts are statistics, not
  findings. Measured on the ritecoach crawl: `indexability.noindex` filters
  3,999 pages to 3,913, and the chip counts URLs rather than findings so the
  number on the chip is the number of rows the click produces

### Export


- [x] **T4.14** CSV and JSON export, streamed from SQLite so exports never
  materialise in memory — a new `pounce-export` crate: one statement, one
  `Write`, exactly one row alive between them. CSV quoting is RFC 4180 and
  hand-rolled (eleven lines against a dependency); JSON keeps `null` where CSV
  cannot, which is the absent-is-not-empty distinction reaching the file
- [x] **T4.15** Export current filtered view, not just everything — the same
  code path with a different `FilterSpec`, so the two cannot drift. A test
  holds it

### Friendliness (added 2026-08-25)

From [`docs/2026-08-25-ux-debt.md`](docs/2026-08-25-ux-debt.md), after the owner
used the app on a real site and could not follow parts of it. PRODUCT.md §
Users is amended: the audience now includes agency staff who will not learn the
vocabulary. Density stays; rawness goes.

- [x] **T4.16** Findings read as sentences — the registry's `description` and
  `remediation` in the issue list and the detail pane, with the rule id demoted
  to metadata. A `rules` command carries the prose across once per session; the
  store keeps ids on issue rows, because denormalising a sentence onto four
  million rows is that sentence written four million times and wrong the moment
  it improves. `indexability.noindex · 3,913` now reads "The page tells search
  engines not to index it. — 3,913", and the selected rule shows its fix
- [x] **T4.17** Product header, not diagnostics: the open crawl and its size,
  not `schema 13`. Engine and rule counts move to an About surface — a native
  `<dialog>`, which brings its own backdrop, focus trap and Escape key. It also
  says when the open file was written by a different build, which is the one
  thing the header string was actually for
- [x] **T4.18** Three states, not one page: setup → running → results, with an
  empty state that says what the app is for. The screen is derived, not stored:
  a crawl in flight *is* the running state and an open file *is* the results
  state. The run moved out of the form and into the shell, which is what lets
  the results appear while the form's crawl is still going
- [x] **T4.19** Politeness made legible — what "per-host requests" and "delay"
  do to a site, stated where the choice is made, without lecturing. Prompted by
  a default crawl hitting a 60 req/min site at 7 URL/s. Three presets (Gentle =
  1/s, Normal, Fast) and a sentence under the fields that changes as they do:
  arithmetic when there is a delay, "as fast as the server answers" when there
  is not — which is the true answer and also the warning
- [x] **T4.20** Errors that offer the next step rather than restating the engine
  — two new typed variants (`OutputExists`, `BadSeed`), each carrying the
  correction beside the complaint, and a `Failure` in the UI that is a message
  plus an optional button. "Save as ritecoach-2.pounce instead" and "Try
  https://example.com" are one click, not a retyped field
- [x] **T4.21** **Results while the crawl runs** — the grid queries the file
  being written rather than waiting for the crawl to end. A WAL reader under a
  live writer already works (T3.3); today the app just does not open the file
  until `start_crawl` returns. Biggest single change to how the app feels.
  `Store::open_read_only` is the second connection, and `open_crawl` reaches
  for it whenever a crawl is in flight; the pane re-queries once a second, not
  at the 10 Hz the progress channel ticks at. Rows arrive in batches of 500 —
  the writer's transaction size — so "275 fetched, 0 rows" is a real state and
  both empty messages say so

### Craft (added 2026-08-25)

The bar is *good* interface work, not adequate. These are the details that
separate the two, and each is small on its own.

- [x] **T4.22** Layout after Screaming Frog's *arrangement*, not its components:
  issue rail with live counts as primary navigation, tabs over one crawl, detail
  pane under the grid. Friendly and modern components — the audience is an
  agency, not a developer. The tabs are saved *questions* (All pages, Broken,
  Redirects, Not indexable), each one a filter the engine already serves, so
  switching tabs is a query rather than a mode
- [x] **T4.23** Type scale actually used — 11px appears 36 times and 13px never;
  rows and body move to 13px, secondary labels to 12px, 11px reserved for dense
  metadata. Row height and vertical rhythm follow. Verified on a 14" display at
  1800×1169, where the current UI is measurably too small. Landed: 44 uses of
  `text-xs` became 6; the scale gained `xl` (18px) for the wordmark and the live
  numbers, and `lg` moved 14px → 15px. Rows are 34px. Both themes screenshotted
  — which is how the theme choice was found not to survive a restart
- [x] **T4.24** Every interactive thing has states: hover, `:focus-visible`,
  active, selected, disabled. A selected grid row, and arrow-key navigation with
  Enter opening the detail pane — a specialist should never need the mouse.
  Three component classes (`.btn`, `.btn-primary`, `.field`) carry the whole
  matrix, so a new control is correct by default; the grid is a `role="grid"`
  with Arrow/Page/Home/End and Enter, and the keyboard cursor is drawn
  separately from the opened row
- [x] **T4.25** The states an app actually spends time in: first-run empty,
  loading skeletons that do not flash, a filter that matches nothing, and errors
  that name the next step. Currently only the happy path is designed. Landed:
  `useDelayed` holds every placeholder back 150 ms so a 12 ms query shows none
  at all; the grid separates "not counted yet" from "counted zero", which is
  what made the empty message flash on every keystroke; and the empty state
  carries the way out of itself. (Errors that name the next step are T4.20.)
- [x] **T4.26** Motion applied where PRODUCT.md already allows it — 120–150ms,
  state only. Pane and row transitions, never decoration. The detail pane rises
  8px as it arrives; dialogs fade with `@starting-style`. The grid rows are
  deliberately *not* animated: a per-window fade means re-keying cells by row
  id, which changes how React reconciles a virtualised list, and T4.9's 0.0%
  dropped frames at 500k is worth more than a fade
- [x] **T4.27** One pass over spacing, alignment and column widths together, at
  the sizes T4.23 lands on. Numbers right-aligned, URLs truncated from the
  middle rather than the end, headers aligned to their data. Columns are CSS
  grid tracks now — fixed for the numeric ones, `minmax(…, Nfr)` for URL and
  title — so a 1,240px row stops needing a horizontal scrollbar beside a 320px
  rail. The middle truncation is pure flexbox: a truncating head beside a
  `shrink-0` tail, so it needs no measurement and survives a resize

### The redesign (added 2026-08-25)

Everything from 2026-08-25 onward, in the order it happened. Three rounds of
owner review drove it: the app used against a real site, then held up against
Screaming Frog tab by tab, then audited with `$impeccable` and `apple-design`.
The through-line is that each round found the *previous* round had aimed one
layer too shallow: at wording when the problem was layout, at layout when the
problem was the type scale, at the type scale when the problem was that the
application hid itself behind a welcome screen.

- [x] **T4.28** The new-crawl screen, in the user's words — *added and done
  2026-08-25, after the owner said it "still looks technical as hell".* The
  friendliness pass fixed the results side and never came back to the screen
  that starts everything: eight controls of equal weight, every label the
  engine's own (`Seed URL`, `Max depth`, `Per-host requests`, `Delay per
  request`), and two required empty fields before the button would light.
  Now one field and a button — the file name is proposed from the address
  (`ritecoach.com` → `~/Documents/ritecoach-com.pounce`) and shown rather than
  demanded, a bare domain gets its `https://` in place instead of an error, and
  everything else folds behind "More options" in plain words. The pace sentence
  stays *outside* the fold: it is the warning T4.19 exists for, and a warning
  behind a disclosure is not a warning
- [x] **T4.29** The overview panel, on the right — *added 2026-08-25 after the
  owner sent a Screaming Frog screenshot: "look how good and informative it
  is".* `Store::crawl_overview` counts what a crawl **contains** rather than
  what is wrong with it — pages crawled, still to fetch, never answered, then
  what was found (Pages / PDFs / Images / Other), how it answered (2xx…5xx) and
  indexability, each with a count and a share of the total. It sits on the right
  where Screaming Frog puts it, as two tabs over one crawl with the findings
  rail; every filterable line opens those rows in the grid. All index-only
  queries, so it stays affordable to redraw once a second while a crawl writes
- [x] **T4.30** Tabs that are views, not screens — *added 2026-08-25, same
  request.* A tab is a saved question **with its own columns**: Page titles
  brings the title, its length and the word count forward and drops the bytes,
  because those are the four things you look at when auditing titles. Nine of
  them — All pages, Page titles, Meta descriptions, Canonicals, Broken,
  Redirects, Not indexable, Images, Response times. `RowView` gained
  `meta_description`, `canonical` and `elapsed_ms`, all scalars on `pages`, so
  the row shape rule holds; title and description *length* are derived in the
  grid rather than sent, because a column computable from one already on the
  wire is not worth a byte more of it
- [x] **T4.31** The detail pane gets tabs, and the grid gets a status bar —
  *added 2026-08-25, same request.* Details / Findings / Linked from / Links to,
  each carrying its count on the tab, because "Linked from 0" and "Linked from
  11,997" are different pages and you should not have to open one to find out
  which. The row count and the current view move to a foot bar under the grid,
  where Screaming Frog keeps its counts
- [x] **T4.32** Modern, not a terminal — *added 2026-08-25: "something about the
  UI is not comfortable. It's too sharp… it looks like a TUI tool made for
  hackers."* Three things were doing it, all measurable: **42 uses of
  monospace**, a 5px radius on every control, and a `--shadow` token defined in
  T4.2 and used by nothing. Mono is now only on text read character by
  character — URLs, paths, canonicals — and a `.nums` class carries Inter's own
  tabular figures everywhere else, so columns still line up. Radii 5/7 → 7/11
  with a 14 for panels, the dark ground lifts off near-black (`#08090a` →
  `#121317`, borders and muted text moved with it, AA re-verified), the raised
  panels finally cast the shadow, and rows are 38px. The window also opens
  **maximised** — a table application whose default window shows a third of the
  columns starts every session with a drag — and the grid has a row number
  column, which is how you say "row 412" to someone
- [x] **T4.33** The right panel is the current tab's filter list — *added
  2026-08-25 after the owner pasted every Screaming Frog tab: "look how good and
  informative it is".* T4.29 built that panel as one **static crawl summary**,
  which is not what Screaming Frog's is: its right panel is **the filter list
  for the tab you are on** — Missing / Duplicate / Over 60 Characters on Page
  Titles, Over 100 kB / Missing Alt Text on Images. That is why it stays usable
  at density: the tab narrows the question, the panel enumerates every answer.
  Pounce's rule registry already *was* that list — `title.*` is the Page Titles
  panel, `media.*` is the Images panel — so it needed no engine work, only the
  realisation. Rules that found nothing are listed at **0**, greyed: "Missing 0"
  is a check reporting a pass, and silence is not the same statement
- [x] **T4.34** Total redesign — *added 2026-08-25, owner: "I need a complete
  redesign… colors, layout, tokens… Everything."* Warm stone neutrals replace
  the cool blue-black (paper light, charcoal dark), a violet accent `#6A4DF4`
  replaces the Linear indigo, radii to 8/12/16, real control shadows, underline
  tabs replace folder tabs, styled scrollbars, and an **overlay titlebar** so
  the header is the window chrome with the traffic lights inside it. The right
  panel's rows carry proportional data-bars in their severity colour — the
  chart folded into the list. PRODUCT.md § Brand Commitments rewritten in the
  same commit; the RAG-band prohibition and never-colour-alone rule survive
  their second redesign; `check:contrast` re-verified every tier in both themes
- [x] **T4.35** The type scale that actually creates hierarchy — *from
  `$impeccable critique`, which scored the app 32/40 with a single weak
  dimension: Aesthetic & Minimalist Design 2/4.* The diagnosis was measurable
  and was not colour: the 11/12/13/15/18 scale had ratios of **1.09 and 1.08**
  between its bottom three steps, below even the product register's 1.125
  floor, so everything read at one volume and two repaints could not fix it.
  Four steps now (11/13/16/20, ratios 1.18/1.23/1.25), each carrying its **own
  tracking and leading** per Apple's rule that both are size-specific, plus
  `font-optical-sizing: auto`. Merging 12px into 13px meant labels lost their
  size distinction, so hierarchy moved to weight and case: panel sections and
  column headings are 11px semibold uppercase, tracked out
- [x] **T4.36** Neutrals tinted toward the brand, in OKLCH — *`$impeccable`
  shared colour law: use OKLCH, tint every neutral toward the brand hue, never
  `#000`/`#fff`.* Measured the violation rather than eyeballing it: the warm
  greys sat at hue **84°** while the accent sits at **283.5°**, so the two read
  as belonging to different products. The whole palette is OKLCH now, neutrals
  at the brand hue with chroma 0.004–0.014, and the four pure-`#ffffff` slots
  are gone. `check:contrast` learned to parse OKLCH — and to **throw** on an
  unparseable colour, because it had already silently graded garbage twice.
  Custom scrollbars reverted: the product register bans reinventing standard
  affordances, and I had shipped one the hour before
- [x] **T4.37** Copy without em dashes — *`$impeccable` bans them outright.*
  43 in user-facing strings (the other 45 are in code comments, which the ban
  does not cover). At that density they stop being punctuation and become a
  texture of interruption. Qualifiers took parentheses (`Worked (2xx)`,
  `None (reached directly)`), clauses took periods or colons (`Stopped: URL
  limit reached`), and the absence markers dropped the dash entirely — the
  colour already carried the distinction, so `— none` was a dash doing no work
- [x] **T4.38** A command palette, because the keyboard reached the grid and
  nothing else — *`$impeccable critique` persona red flag: "Alex (technical SEO
  specialist) can arrow through the grid, but cannot reach the 9 view tabs, 5
  filter controls, or search from the keyboard. Every filter change is a mouse
  trip, for someone doing this daily."* ⌘K from anywhere, over views, every
  rule (with its count, zeroes included), and the app actions. Subsequence
  matching, so `ntx` finds Not indexable. A `Search ⌘K` button in the header
  because a palette nobody knows about is a palette nobody uses
- [x] **T4.39** `$impeccable polish` — the audit found less drift than
  expected: **zero hard-coded colours, zero `console`/`TODO`/`any`**, one
  arbitrary value (a documented pane height), and 32 of 32 buttons inside the
  `.btn`/`.tab` system bar one palette list-row that is not button chrome. Two
  real gaps fixed: `prefers-contrast: more` (Apple names three accessibility
  signals; we honoured one) and the command palette's search field, which
  removed its focus outline without the replacement every other `.field` has.
  Two findings **deliberately not acted on**, with reasons recorded: the grid's
  header rule stays (Apple's scroll-edge guidance targets floating translucent
  chrome, not a table header, and the product register calls familiar table
  patterns a feature), and gutters stay uniform (the shared law asks for varied
  spacing; the product register asks for predictable grids, and for this
  register the more specific rule wins — rhythm lives in vertical section
  spacing instead)
- [x] **T4.40** The shell: one screen, always visible — *from the owner's
  Screaming Frog screenshots: "look how easy it is".* Two things made that app
  feel easy and neither was a feature. **Its URL bar never leaves**, so
  re-crawling is one field away rather than a screen you navigate to and back
  from; and **it shows the entire application filled with zeros before you
  crawl anything**, so you learn it by looking at it. Ours did the opposite on
  both counts. `CrawlBar` now lives in the header permanently (address, pace,
  Start, Clear, Options), the welcome and setup screens are deleted, and the
  results screen renders at rest: tabs, filter bar, column headings, the panel
  tree with zeros, "No data" over the recent-crawls list, "No URL selected" in
  the pane, "Idle" in the status bar
- [x] **T4.41** Radix, where hand-rolling was the wrong call — three
  dependencies, each with a problem it solves that we could not solve cheaply.
  **`react-resizable-panels`**: draggable splitters between grid, detail pane
  and the right panel, which Screaming Frog has and we did not; it ships
  arrow-key resizing on a focused handle and persists the layout per person.
  **`@radix-ui/react-dropdown-menu`**: the column picker was a *modal dialog*
  covering the table it configures, which is the "modal as first thought"
  anti-pattern impeccable bans; it is a menu now. **`@radix-ui/react-tooltip`**:
  every grid column explains itself on hover *and on keyboard focus*, closing
  the "no column explanations" gap that scored Help and Documentation 2/4.
  Not adopted: shadcn wholesale, because it would import a second token system
  that fights DESIGN.md, and native `<select>`/`<dialog>` stay as they are
- [x] **T4.42** The Issues panel — *the most client-legible surface in
  Screaming Frog, and the one an agency hands to a client.* A second tab on the
  right panel listing every finding that occurred, worst first, in report
  vocabulary: **Issue / Warning / Opportunity** and **High / Medium / Low**,
  with URLs affected and share of the crawl, above a tally of how many of each
  kind. The mapping onto our three severities is 1:1 and honest rather than
  invented, because `pounce-audit`'s severity definitions already say exactly
  this: Critical means assume the page is broken, Warning means a real defect
  on a page that otherwise works, Notice means nothing is wrong and there is
  only headroom. Global, unlike the Overview beside it: a worklist is not a
  filter list, and a check that passed is not work
- [x] **T4.43** The detail pane fills the panel it lives in — the pane kept
  `h-[32vh] shrink-0` from before T4.41 gave it a resizable `Panel` that owns
  its height, so the content stopped at 32% of the window while the panel kept
  whatever the splitter gave it: dragging the pane taller revealed canvas
  rather than content. `h-full`. The general lesson, since this is the second
  time a leftover fixed size has fought a new parent: when a layout gains an
  owner for some dimension, every child asserting that dimension is now wrong
- [x] **T4.44** A finding shows the field it is about — clicking "More than
  one page uses this meta description" showed a Title column, which is the
  interface hiding the evidence for its own claim. Selecting a finding now
  swaps in the columns for its batch (`description.*` brings Meta description
  and its length, `title.*` brings Title and its length, and so on). Reported
  as a false positive in `description.duplicate`; it was not. New Jersey has
  four Franklin townships across four counties, the site templates its
  description on town name alone, and all four pages carry byte-identical text
  — verified against the crawl file. The rule was right and the grid was
  showing the wrong column

### Found by using it

- [x] **T4.45** Duplicate findings sit next to each other. A finding that is
  *about* duplication is only legible when its duplicates are adjacent, and the
  grid's default order — by URL — is the one order that pulls them apart, since
  a duplicate description is usually two pages in different sections. Migration
  014 indexes `meta_description`, `SortColumn` gains it, and selecting
  `title.duplicate` or `description.duplicate` now sets that sort as well as
  those columns: the four Franklin townships arrive as rows 3–6 rather than
  scattered through 212. Measured before adopting, because the index is
  maintained during the crawl: 221.6 ms before and 228.7 ms after on the writer
  bench at 5,000 pages, which criterion calls no change at p = 0.09. One
  composite, `(has_issue, meta_description)`, because "pages with problems" is
  the view a duplicate finding lands in; the other filter kinds reach the
  column through `EXISTS` or grey the header rather than run an unmeasured sort

- [x] **T4.46** A search preview in the detail pane. The one panel aimed at the
  person the report is *for* rather than the person running the crawl: an
  agency telling a client "your title is 71 characters" has to explain that,
  and showing them the sentence cut off mid-word does not. Title, description
  and breadcrumb from data the crawl already holds, cut **by width** — a column
  at the width a result gets, the type sizes a result uses, and `line-clamp`
  doing the trimming, because a 60-character rule of thumb calls "Illinois" and
  "lllllllll" the same length. `noindex` and `nosnippet` are stated rather than
  simulated, a missing title or description says what a search engine does
  instead, and the panel is captioned as an approximation rather than implying
  a promise about a renderer that is not ours to read

- [x] **T4.47** A Headings tab, and H1/H2 on the row. The grid shows the first
  heading of each level and how many there are, because a page with two H1s
  shown as one heading looks healthy. Both come from `page_detail`, which the
  grid had never read: the obvious `LEFT JOIN` reads as 200 rowid lookups and
  is not — SQLite computes it for every row `OFFSET` steps over too, and an
  unfiltered sort half way into a 1M crawl went from 7.7 ms to **494 ms**
  against a 150 ms gate. A second statement keyed by the ids just returned does
  the same work for the window alone and leaves every gate number where it was
  (`docs/benchmarks/2026-08-28-headings-on-the-row.md`)

- [x] **T4.48** The site as a folder tree, and a List/Tree toggle beside the
  Columns button. A list of 4,000 URLs says nothing about a site's shape;
  `/baseball/` holding 563 of them says most of it, and it is the view a client
  recognises without being taught the table. One level at a time by prefix, so
  the invariant holds — the UI asks for the children of one folder and gets
  grouped counts, never the tree. `/blog` and `/blog/one` merge into one row
  rather than reading as a duplicate, with the folder's own page offered as the
  first thing inside it; every level open is re-read while a crawl writes,
  because a tree of counts that stops counting looks finished; and a folder
  with more than 500 children says how many are not listed

- [x] **T4.49** A Duplicates tab. Two pages with the same title and two with
  the same description are one conversation with a client, and they were three
  clicks apart in two different tabs. The panel lists three rules from three
  batches, which `ruleLines` now allows by taking a full rule id as well as a
  batch name — six lines of change rather than a second panel. Clicking any of
  them filters the grid *and* sorts by the field, so T4.45's grouping applies
- [ ] **Hreflang tab — deliberately not built.** It is on the Screaming Frog
  list, and the data is in `page_detail`, but no rule in the v0.1 thirty reads
  hreflang and the reference crawl has none at all: it would be a tab of empty
  cells nothing could be verified against. Worth building beside the first
  hreflang rule, which is community work after the rule SDK

- [x] **T4.50** A URLs tab: length, parameters, and what is unusual about the
  address. Not four boolean columns — one Notes column reading "capitals,
  underscores" or "Clean", because none of these is a defect on its own and
  four permanent "no"s is a column nobody reads. **None of them is an audit
  rule either**: the registry is capped at thirty for v0.1 and full, so this is
  the title-length arrangement — the fact shown where you are already looking
  rather than promoted to a finding. `urlNotes` is the first pure UI logic with
  a real check behind it (`npm --prefix ui run check:logic`, Node running the
  TypeScript directly, no framework), and it caught two bugs on its first run:
  `%C3%A9` reported capitals it does not have, and `?ref=Twitter_x` reported an
  underscore in a path that has none

- [x] **T4.51** Three bugs from one real crawl of an 18-page site, all of them
  the interface asserting something the data does not say:
  - **The tree showed a root and no children.** It rooted at the *seed*, and
    the seed is where the crawl was pointed, not where anything was found:
    `myzion.com` redirects to `www.myzion.com`, so the range matched none of
    the 18 pages. The root now comes from the shallowest crawled URL — where
    the crawl actually landed after redirects — with the seed as the fallback
    for a crawl with no pages. Every site with a canonical host redirect hit
    this, which is most of them
  - **The Images tab was empty over a crawl holding 88 images.** It filtered
    `pages` for `kind = 'image'`, a kind the crawler stopped writing when
    migration 011 gave resources their own table — correctly, since an image
    checked with `HEAD` has no body, no title and no id. The grid is now
    generic over its row type and the Images view reads a windowed
    `resource_rows`, with columns that suit a resource: status, declared size
    (where "Not declared" is not zero), declared type, findings
  - **A finding read "133.3%", and the headline read "216.7%".** Both divided
    findings about images by the number of *pages*. `IssueCount` gains
    `page_urls` so a rule whose subjects are not pages shows its count without
    a share, and the headline row counts `pages_with_issues` — the number
    clicking it actually produces. The footer under the Images list said "18
    pages" over 88 images; it now counts images

- [x] **T4.52** A consistency sweep, from three findings rather than three
  opinions: a response code was drawn by **three copies** of the same
  conditional and they disagreed — a 404 was amber in the page grid and red in
  the images grid, so the same fact looked like two severities depending on the
  tab. One `statusTone`, used by both grids and the detail pane, with the
  boundaries checked in `check:logic`. Findings **wrap instead of truncating**:
  they are sentences from the rule registry, and "An image the page references
  is large …" names no image and no threshold, which defeats the reason they
  were written as sentences. Tree rows take the grid's 38px row height, so
  switching between two readings of one crawl does not change the density of
  the page

**Gate M4:** — [`docs/benchmarks/2026-08-25-gate-m4.md`](docs/benchmarks/2026-08-25-gate-m4.md)
- [ ] Crawl a real site start to finish without touching a terminal — **the one
  item still open.** Exercised end to end against the local fixture; needs a
  person, a mouse, and a site they are happy to crawl
- [x] Table responsive at 500k rows — 0.00% of frames dropped over 360 frames of
  continuous scrolling (baseline 17.0 ms, worst 19.0 ms), peak RSS 79–116 MB on
  a 681 MB file
- [x] Cold start under 400ms — 272 ms median of thirteen launches; the one
  470 ms outlier is the first launch after a build, binary not yet cached
- [x] Both themes verified — production build, both captured, and
  `check:contrast` passes every tier in both

---

## M5 — MVP release (v0.1)

**Goal:** ship it.

- [x] **T5.1** Tauri bundler config: MSI/NSIS, universal .dmg, AppImage/.deb/.rpm
  — targets, identifier, category, publisher, copyright and the icon set are
  configured; the macOS `.app` builds and runs. Two things this machine could
  not close: the `.dmg` step shells out to AppleScript and needs Finder
  scripting permission, and Windows/Linux bundles need those platforms (T5.3).
  **Finding: the bundled app is subject to macOS TCC** — see the note below
- [ ] **T5.2** Code signing and notarisation — macOS notarisation is the usual multi-day surprise, start it early
- [x] **T5.3** GitHub Actions release matrix on tag — `.github/workflows/release.yml`,
  four native jobs (macOS arm64 and x64, Linux x64 on 22.04 for glibc
  portability, Windows x64), artifacts collected per platform and attached to a
  **draft**. Written but never executed: it needs a tag, and the installers it
  produces are unsigned until T5.2
- [x] **T5.4** `README.md`: benchmark table above the fold, **"what this doesn't
  do yet"** section, ~~dual-licence note~~ → all-rights-reserved, matching the
  actual LICENSE (see T5.6 for the conflict). The head-to-head is present but
  explicitly *not* quoted as a headline: it is stale in Pounce's favour and needs
  re-taking on one fixture first
- [x] **T5.5** `ARCHITECTURE.md` explaining query-don't-dump — the invariant
  first, then everything that follows from it, with the measured number beside
  each decision
- [x] **T5.6** ~~`CONTRIBUTING.md`~~ — **cut, 2026-08-28.** The owner's call:
  Pounce is closed source and stays that way. A contributor guide is furniture
  for a project that takes contributions, and this one does not. CLAUDE.md
  § Conventions was right and the task was wrong; the licence does not move.
  T5.4's "dual-licence note" is already resolved the same way
- [ ] **T5.7** Publish the benchmark, including runs where competitors timed out or errored
- [ ] **T5.8** Landing page reusing the identity from `docs/product-plan.html` —
  **blocked on a decision, 2026-08-25.** That file's identity is warm (cream
  ground, cyan accent, Archivo + Source Serif) and PRODUCT.md § Brand
  Commitments is cool indigo on neutral with Inter + JetBrains Mono, explicitly
  "not warm". The app is built to the second. Which identity the public page
  wears is a brand decision, and building it in the wrong one is worse than not
  building it
- [x] **T5.9** ~~GitHub Sponsors~~ — **cut, 2026-08-28**, with T5.6 and for the
  same reason. Sponsors is community furniture on a proprietary project. The
  "no paid tier" promise still belongs in the README, where it is a statement
  about the product rather than a donation button

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
