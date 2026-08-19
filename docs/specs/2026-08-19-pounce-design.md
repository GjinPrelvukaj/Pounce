# Pounce — Design Spec

**Date:** 2026-08-19
**Status:** Approved (design). Implementation plan not yet written.
**Presentation version:** [`docs/product-plan.html`](../product-plan.html)

---

## 1. Problem & premise

Screaming Frog is the incumbent technical-SEO crawler: Java/Swing, £199/yr, 500 URLs free. For a developer audience it is hard to script, hard to put in CI, and heavy to run.

The obvious response — "a free, cross-platform, unlimited-URL alternative" — is **no longer differentiated**. As of 2026 at least four projects already occupy that position:

| Project | Stack | Delivery | Scale | License |
|---|---|---|---|---|
| Screaming Frog | Java + Swing | Desktop | RAM or DB mode | £199/yr, 500 free |
| FreeCrawl | Electron + Node + Playwright | Desktop | 1M+ URLs | MIT-style, free |
| LibreCrawl | Python + Flask + Playwright | localhost web | 5M URLs | MIT, free |
| Sitebulb | .NET | Desktop + cloud | Large | Paid |

FreeCrawl in particular already ships disk-backed SQLite, a headless CLI, 200+ checks, GSC/GA4 integrations, and an MCP server, with no paid tier planned.

**The remaining gap:** none of them is natively compiled. Every incumbent pays a runtime tax (Electron's bundled Chromium, Python's GIL, the JVM heap) on exactly the two operations a crawler performs most — parsing HTML and holding a large result set. That gap is structural and cannot be closed without a rewrite.

## 2. Positioning

> **Pounce crawls faster, on less memory, from a smaller binary than anything else — and every claim ships with a reproducible benchmark.**

Three non-negotiable consequences:

1. **Benchmarks are a feature.** The harness is built in Phase 0, before any crawler code.
2. **Feature parity is explicitly not the goal.** v0.1 caps at ~30 audit rules. Racing a feature list we are behind on is the primary failure mode for a solo project.
3. **Tradeoffs resolve toward speed.** When "nicer UX" conflicts with "faster crawl", faster wins and the UX is solved another way.

**Known risk:** the speed advantage may be imperceptible on the sub-50k-URL sites most people crawl. Mitigation is to lead published benchmarks with the metrics users *feel* — peak memory, cold start, sort latency on 500k rows — not throughput alone.

## 3. Name & identity

**Name:** Pounce. Short, a verb, what jumping spiders do, reads as speed before arachnid.
**Must verify before first release:** crates.io, npm, PyPI, GitHub org, `pounce.dev` / `pounce.sh`.

> **⚠ Superseded 2026-08-19.** The palette and typography below were replaced. The application's visual direction is now **Linear/Apple lineage** — cool neutrals, indigo accent, Inter + JetBrains Mono. See `PRODUCT.md` § Brand Commitments for the authoritative identity, and `design/crawl-view.html` for the reference implementation. Everything else in this spec still stands.

**Design direction (superseded):** *measurement instrument* — oscilloscope, not dashboard. The product's claim is a number, so the identity should look like something that produces trustworthy numbers.

### Palette

| Token | Hex (dark) | Role |
|---|---|---|
| Housing | `#14110E` | Ground. Warm/brown-biased, deliberately not blue-black. |
| Panel | `#1C1815` | Raised surfaces — tables, cards, sidebars. |
| Trace | `#2ED3DE` | The single accent. Primary actions, active state, focus rings, sparklines. |
| Readout | `#F2EBE1` | Primary text. |
| Bezel | `#857A6E` | Tertiary text, column labels, disabled state. |
| Rule | `#2E2823` | Gridlines, borders, dividers. |

Light-theme counterpart: ground `#F4F0EA`, trace darkened to `#0A7C86` for contrast.

### Severity — deliberately outside the brand hue

| State | Hex | Use |
|---|---|---|
| Critical | `#F2666B` | 5xx, redirect loops, noindex on important pages |
| Warning | `#E5A93F` | Long titles, missing alt, thin content |
| Notice | `#6FA8F5` | Informational |
| Pass | `#67C15E` | Used sparingly — passes render neutral grey in bulk views |

**Rationale:** a crawler UI is dominated by severity states. A brand colour inside the red–amber–green band would collide with meaning on every screen. Cyan is chosen precisely to leave that band free. Severity must never be encoded by colour alone — always pair with icon and label.

### Typography

- **Archivo** — display, headings, UI chrome
- **Source Serif 4** — long-form (docs, release notes, website)
- **JetBrains Mono** — all data: URLs, status codes, byte counts, timings. Tabular figures mandatory so columns align.

### Mark

An orb web reduced to radial threads, right-hand threads brightened so they read as motion lines; centre node is the crawler. Works at 16px, mono-colours cleanly, never drawn as a literal spider.

### Voice

- Numbers over adjectives. Not "blazingly fast" — "412 URLs/sec on a 4-core laptop, here's the harness."
- Never name competitors negatively. Publish the benchmark, include them, let the table speak.
- README carries a "what this doesn't do yet" section above the fold.

## 4. Stack

| Layer | Choice | Rationale |
|---|---|---|
| Language | Rust (2024 edition) | No GC pauses mid-crawl, fearless concurrency, single-binary distribution |
| Async runtime | tokio | Multi-threaded work-stealing; the frontier maps onto it naturally |
| HTTP | reqwest + hyper | Pooling, HTTP/2, gzip/brotli/zstd. **Auto-redirect disabled** — the chain is data |
| HTML parse | lol_html | Streaming, single-pass, no DOM construction. Biggest single perf win. `scraper`/html5ever only where tree access is genuinely required |
| Rate limiting | governor | GCRA, per-host. Politeness is a correctness requirement |
| Frontier dedup | DashMap + custom queue | Sharded map, no global mutex on the hottest path |
| Storage | SQLite via rusqlite (bundled) | WAL, batched transactions, disk-backed from row one. Resumable crawls and portable `.pounce` files fall out free |
| App shell | Tauri 2.10 | OS webview, no bundled Chromium (~10–15 MB installers) |
| UI | React 19 + TypeScript | TanStack Table + TanStack Virtual — most battle-tested virtualised grid |
| Styling | Tailwind v4 | Tokens map 1:1 to the palette; no runtime cost |
| JS rendering (v0.3) | chromiumoxide | CDP driver. Detects installed Chrome rather than bundling |
| Packaging | Tauri bundler + GH Actions matrix | MSI/NSIS, universal .dmg, AppImage/.deb/.rpm |

## 5. Architecture

Cargo workspace of small, independently testable crates. The GUI is one consumer of the engine, never its owner.

```
pounce/
├─ crates/
│  ├─ pounce-core      orchestrator, frontier, scheduler, crawl lifecycle
│  ├─ pounce-http      fetch pool, retries, redirect chains, robots.txt, rate limits
│  ├─ pounce-parse     lol_html extraction → PageRecord
│  ├─ pounce-store     SQLite schema, batched writer, query API, resume state
│  ├─ pounce-audit     rule registry; each check is one testable unit
│  ├─ pounce-export    CSV / JSON / XLSX / sitemap XML
│  ├─ pounce-bench     benchmark harness + fixture site (Phase 0)
│  ├─ pounce-cli       headless binary, CI exit codes
│  └─ pounce-app       Tauri commands + events (thin)
└─ ui/                 React + TanStack, virtualised grid
```

### 5.1 The load-bearing decision: query, don't dump

The intuitive design — crawl into memory, serialise results over Tauri IPC, render in React — falls apart around 100k rows and would make a Rust app feel slower than the Electron incumbent. **This is the single most likely way the project fails.**

Instead, the UI holds **no crawl data**. It issues queries; SQLite returns only the visible window.

```rust
// The UI asks for what is on screen. Nothing more, ever.
#[tauri::command]
async fn query_rows(
    crawl: CrawlId,
    filter: FilterSpec,      // compiled to a WHERE clause
    sort: SortSpec,          // indexed columns only
    offset: u32, limit: u32, // limit ≈ 200, the viewport
) -> Result<Page<RowView>>;

// Progress on a channel, throttled to 10 Hz — not one event per URL.
#[tauri::command]
async fn subscribe_progress(ch: Channel<CrawlProgress>);
```

Consequences: sorting and filtering become SQL over indexed columns (milliseconds on millions of rows); scroll position maps to `OFFSET`; memory stays flat regardless of crawl size; the app stays responsive mid-crawl because WAL separates reads from writes.

This must be proven with a 500k-row seeded database **before** any UI is built on top of it.

### 5.2 Crawl pipeline

```
frontier (priority queue, deduped)
  → fetch pool (N workers, per-host rate limits)
  → parse (streaming, blocking thread pool)
  → batched writer (transactions of ~500 records)
  → audit rules (incremental, not deferred to end)
```

Bounded channels between every stage, so a slow disk throttles the fetchers rather than ballooning memory.

### 5.3 Politeness (non-negotiable)

robots.txt honoured by default; per-host concurrency caps; configurable delay; honest identifiable user-agent carrying a project URL; `Retry-After` respected. Aggressive settings are opt-in and clearly labelled. A fast crawler that gets its users IP-banned is a liability.

## 6. MVP — v0.1

**Target:** a 100k-URL crawl, end to end, with a table that is genuinely workable. ~10–14 weeks solo, and only if scope holds.

### In scope

- Crawl from seed URL with depth, URL-count, and time limits
- robots.txt parsing; sitemap.xml discovery and ingestion
- Status codes, full redirect chains, response time, size, depth
- Internal/external classification, nofollow handling
- Extraction: title, meta description, H1/H2, canonical, meta robots, hreflang, Open Graph, word count, images + alt
- Inlink/outlink graph per URL
- ~30 audit rules
- Virtualised table: sort, filter, column picker, detail pane
- Issue overview with counts, drilling into a filtered table
- CSV + JSON export
- Save, close, reopen, resume a crawl
- Signed builds for Windows, macOS, Linux
- Published, reproducible benchmark

### Explicitly out of scope for v0.1

JavaScript rendering · GSC/GA4/third-party integrations · scheduled crawls · link graph visualisation · custom extraction (CSS/XPath/regex) · crawl diffing · PageSpeed/CWV · log file analysis · structured data validation · spelling & grammar · auto-update · plugin system

## 7. Roadmap

| Phase | Duration | Content |
|---|---|---|
| **0.0** | 2 wks | **Benchmark harness first.** axum fixture site generating 100k pages with realistic link density plus pathological cases (redirect loops, 30s responses, 5MB pages, malformed markup, robots edge cases). Criterion parse benches. Runner measuring URLs/sec, peak RSS, wall time for Pounce and each competitor identically. Nothing else is built until this works. |
| **0.1** | 10–14 wks | MVP as scoped above. Ships with first published benchmark and an honest README. |
| **0.2** | 6 wks | CLI and CI: `pounce crawl --config pounce.toml --fail-on critical`, JSON reports, meaningful exit codes, published GitHub Action. |
| **0.3** | 8 wks | JavaScript rendering via chromiumoxide, opt-in per crawl, own concurrency budget, detects installed Chrome. |
| **0.4** | 6 wks | Custom extraction (CSS/XPath/regex), XML sitemap generation, crawl-to-crawl diffing. |
| **0.5** | 8 wks | GSC + GA4 joins, log file ingestion, link graph visualisation, 1M → 10M URLs per crawl. |
| **1.0** | — | Rule SDK: custom audit rules as WASM modules or embedded scripting. |

## 8. Performance targets

Design targets, **not measurements** — nothing has been built or measured.

| Metric | Target | Why it matters |
|---|---|---|
| Crawl throughput | ≥ 2× fastest incumbent | Headline claim; measured against local fixture so network noise cannot be blamed |
| Peak RSS @ 500k URLs | < 400 MB | The number users feel; where Electron and JVM tools balloon |
| Cold start to first byte | < 400 ms | Perceived speed |
| Sort 500k rows | < 150 ms | Proves the query-don't-dump architecture |
| Installer size | < 20 MB | Trivially demonstrable, instantly credible |
| Idle memory | < 80 MB | Cost of leaving the app open all day |

## 9. Testing

- **Golden-file parser tests.** Corpus of real-world HTML including badly broken markup, with committed expected `PageRecord` output. Parser regressions silently produce wrong audits — highest-consequence bug class.
- **Fixture site is the integration test.** Same axum server built for benchmarking serves deterministic pathological cases; crawl it and assert the exact resulting graph.
- **One test per audit rule** — a fixture that triggers it and one that does not. Non-negotiable; this is what stops rule count becoming rule debt.
- **Property tests on URL normalisation** — relative resolution, trailing slashes, case, ports, punycode, fragments, session IDs. Where crawlers quietly loop forever.
- **Criterion benches in CI with regression thresholds.** Throughput drop >10% fails the build. When speed is the product, a perf regression is a broken build.

## 10. Project

> **⚠ Revised 2026-08-19.** The project is **closed-source and proprietary**, not open source. The open-source plan below is superseded.

- **Licence:** all-rights-reserved (`LICENSE`). The repository is private. Crates set `publish = false` and carry no `license` field; nothing goes to crates.io. Open-sourcing later remains possible and is deliberately not foreclosed.
- **Repository:** monorepo. `ARCHITECTURE.md` explains query-don't-dump, because it is the decision most easily undone by someone who does not know why it exists. Conventional commits, GH Actions matrix.
- **Sustainability:** undecided. No paid tier is committed to, and — importantly — no "free forever" promise may be published while that remains open. Walking such a promise back later costs far more than never making it.
- **Distribution:** undecided. Signed installers from private releases when there is something to ship.

*Superseded:* dual MIT/Apache-2.0 licensing, GitHub Sponsors, public contributor docs, and package-manager distribution (Homebrew, winget, Scoop, AUR, `cargo install`).

## 11. Risks

| Risk | Severity | Mitigation |
|---|---|---|
| Speed advantage real but imperceptible on typical sites | High | Lead benchmarks with memory, cold start, UI responsiveness — not just URLs/sec |
| Scope creep; SEO checks are infinite | High | Rule registry with hard v0.1 cap of 30. New rules only after the SDK exists |
| Rust + Tauri learning curve stalls momentum | Medium | Build `pounce-cli` against the core before any GUI. A working CLI at week 6 sustains morale |
| IPC/table perf makes the Rust app feel slow anyway | Medium | Query-don't-dump from first commit; prove on 500k seeded rows before building UI |
| Solo maintainer burnout | Medium | Ship v0.1 narrow and early. Public roadmap that says no. Sponsors from day one |
| Incumbents add the missing features first | Low | They can add features; they cannot stop being Electron and Python without a rewrite |

## 12. Immediate next actions

1. **Clear the name** — crates.io, GitHub org, npm, domain. One hour; unblocks everything.
2. **Build the fixture site and benchmark runner** before any crawler code. If the numbers do not come out ahead on a synthetic 100k-page site, the premise is wrong and that is worth learning in two weeks rather than six months.
3. **Spike the storage layer** — seed SQLite with 1M synthetic rows, prove sort-and-filter round trips under 150 ms through a real Tauri command.

---

*Competitive details reflect public project documentation as of 2026-08-19 and should be re-verified before launch.*
