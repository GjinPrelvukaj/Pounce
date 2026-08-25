# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Stack

Decided in `docs/specs/2026-08-19-pounce-design.md`: Rust engine (tokio, reqwest, lol_html, rusqlite) behind a Tauri 2 desktop shell. UI is React 19 + TypeScript + Tailwind v4, with TanStack **Virtual** for the results grid — Table was evaluated in T4.9 and dropped, because sorting, filtering and pagination are all server-side and the row model only ever holds the visible window. The webview means UI work is ordinary web development, but it ships as a native desktop app, not a website.

## Users

**Primary: technical SEO specialists.** Professionals who crawl sites as a daily job, already use Screaming Frog or Sitebulb, and read status codes, canonicals, and hreflang fluently. They want information density, keyboard speed, and familiar conventions.

**Primary, added 2026-08-25: marketing agency staff.** The person who opens a crawl may be an account or content lead rather than a specialist. They can read a table and a percentage; they cannot be assumed to read `indexability.canonical-elsewhere`, know what a canonical is, or infer that "per-host requests: 4" is what gets a client's site to block them.

**Secondary: developers who own SEO for their own product.** Comfortable with HTTP, less fluent in SEO vocabulary.

**What changed, and why it is written down.** The Users section previously said "not onboarding or explanation", and the interface was built to it. On 2026-08-25 the owner used the app against a real site and could not follow parts of it — the strongest available signal, since none of the usual excuses apply to the person who specified it. The audience now includes people who will not learn the vocabulary, so **findings must read as sentences, not identifiers**, and a control that can get a site blocked must say so. Density is not the problem and is not being traded away: a grid that shows a lot at once is still correct. See [`docs/2026-08-25-ux-debt.md`](docs/2026-08-25-ux-debt.md) for the specific failures behind this.

## Product Purpose

Crawl a website and surface its technical SEO defects — broken links, redirect chains, missing or duplicate metadata, indexability problems, image and link issues — across sites large enough that existing tools become slow or unusable. Success is a specialist choosing Pounce over Screaming Frog for a large crawl because it finishes sooner and stays responsive while it does.

## Positioning

Pounce is natively compiled; every competitor is not. Screaming Frog runs on the JVM, FreeCrawl is Electron and Node, LibreCrawl is Python and Flask. Each pays a runtime tax on the two operations a crawler performs most — parsing HTML and holding a large result set — and none can shed it without a rewrite.

The claim is therefore narrow and falsifiable: **faster crawls, less memory, smaller binary, with every number backed by a reproducible public benchmark.** Feature parity is explicitly not the goal.

## Operating Context

Users run one main site repeatedly over time, plus ad-hoc audits of other sites. Both matter: a recents list and frictionless new-crawl setup are first-class, and crawl history makes "what changed since last week" a natural later feature rather than a bolt-on.

A crawl is long-running — minutes to hours. The app is left open, checked periodically, and must stay responsive while crawling. Results are worked through after the crawl finishes: filtered, sorted, drilled into, and exported.

## Capabilities and Constraints

- Crawls of 1M+ URLs, disk-backed in SQLite from the first row. Crawls are resumable and saved as portable `.pounce` files.
- **The UI never holds the dataset.** It queries SQLite for the visible window (~200 rows); sorting and filtering are SQL over indexed columns. This is the load-bearing architectural constraint and it shapes what the interface can offer — any feature requiring the full result set client-side is not buildable.
- Progress events are throttled to ~10 Hz, so live counters update smoothly rather than per-URL.
- v0.1 ships ~30 audit rules, not the 200+ competitors advertise. Deliberate.
- Windows, macOS, and Linux, from one codebase.
- Politeness defaults (robots.txt, per-host rate limits) are correctness requirements, not preferences.
- v0.1 excludes: JavaScript rendering, Search Console and GA4 integrations, scheduled crawls, link graph visualisation, custom extraction, crawl diffing, log file analysis. Export is CSV and JSON; XLSX and sitemap XML are not built.

## Brand Commitments

Name: **Pounce.** The visual direction is **warm and modern** — paper and charcoal, not blue-black; a data application that reads as a product, not as a terminal. Redesigned 2026-08-25 at the owner's direction, replacing the Linear-lineage cool stack ("it looks like a TUI tool made for hackers"). The token file `ui/src/index.css` is the living form of this section.

- **Colour is authored in OKLCH**, and **every neutral is tinted toward the brand hue, 283.5°.** The first warm attempt put the greys at hue 84° (orange) while the accent sat at 283° — nearly 200° apart, so the neutrals and the accent read as two different products. Chroma stays 0.004–0.014: never dead neutral, never visibly purple. There is no pure `#ffffff` anywhere; a white that belongs to no palette is the one surface that gives a tinted UI away.
- **Neutral ramp.** Light: canvas `oklch(96.8% 0.005 283.5)`, surfaces 99.2% / 94.6%, borders 90.8% / 84.0%. Dark: canvas 20.5%, surfaces 23.5% / 27.0%, borders 32.5% / 40.5%. Both ship; the window follows the system.
- **Text:** light 23.5% / 45.0% / 53.5%, dark 95.0% / 71.5% / 62.5%, all at the brand hue. Every tier clears 4.5:1 in both themes — enforced by `npm run check:contrast`, which parses OKLCH, throws on an unparseable colour rather than scoring it, and fails the build otherwise.
- **One accent: violet** `oklch(55.4% 0.2358 283.5)` for fills and primary actions; 49.5% (light) / 74.4% at hue 290 (dark) when the accent must be text. It sits outside the red–amber–green band — the constraint that has now survived two redesigns: severity states dominate this interface and the brand colour must never collide with them.
- **Severity** (critical / warning / notice / pass) is pinned and converted, not changed: light `oklch(53.5% .165 27)` / `(51.6% .103 82)` / `(45.1% .079 222)` / `(46.1% .107 154)`; dark `(77.9% .113 27)` / `(80.3% .135 86)` / `(78.1% .104 214)` / `(75.9% .138 159)`. Notice is cyan rather than blue-violet specifically so it cannot be confused with the accent. Severity is never encoded by colour alone — always an icon and a word beside it.
- **Type: Inter** for all UI, with tabular figures (`.nums`) wherever numbers must align. **JetBrains Mono only for text read character by character** — URLs, paths, canonicals — never for ordinary counts; forty-two monospaced fragments in one window is what a terminal looks like.
- **Four type steps, never five:** 11px dense metadata, 13px body and rows, 16px section headings, 20px wordmark and live numbers. Ratios 1.18 / 1.23 / 1.25. The previous 11/12/13/15/18 scale put three steps within 2px of each other, so every screen read at one volume and no repaint could fix it. **Each step carries its own tracking and leading** — tracking goes slightly positive at 11px and negative as text grows, leading tightens inversely — because one global value is wrong for at least one size by definition. Hierarchy comes from weight and case as much as size: panel and column labels are 11px semibold uppercase, tracked out.
- **Radius** 8 / 12 / 16px (controls / containers / panels). **Depth is real:** controls carry a hairline shadow, panels and dialogs cast `--shadow` — flat rectangles ruled onto one plane read as a TUI. **Motion** 120–150ms, conveying state only.
- **Standard affordances are never reinvented.** No custom scrollbars, no bespoke form controls, no hand-rolled modals — the platform's own versions already respect `color-scheme`, work on all three OSes, and improve without us. A restyled scrollbar shipped in T4.34 and was reverted for exactly this reason.
- Voice: numbers over adjectives. Never "blazingly fast" — state the measurement.

*Superseded:* the Linear-lineage cool indigo world (this section's own previous text, demonstrated in `design/crawl-view.html`), and before it the warm-graphite cyan "measurement instrument" of `docs/specs/2026-08-19-pounce-design.md` §3 and `docs/product-plan.html`. All are stale on visual identity only; architecture and scope still stand.

## Evidence on Hand

**No performance measurements exist.** Every figure in the spec is a design target, not a result. Nothing has been built. Future work must never present these as measured, and no benchmark table may be shown as real until Phase 0 produces one.

No users, testimonials, download counts, or case studies exist. Do not fabricate any.

Real assets: the design spec, the master plan (`PLAN.md`), the Phase 0 implementation plan, and the rendered product plan page.

## Product Principles

1. **Speed is the product.** Where responsiveness and richness conflict, responsiveness wins and the richness is solved another way.
2. **Density over hand-holding, but never rawness over meaning.** Show a lot at once — that is right for the work. It does not license showing a rule id where a finding belongs, or an engine's vocabulary where a product's belongs. Amended 2026-08-25; the original read "The primary user is an expert doing a familiar job. Show more, explain less."
3. **Every claim is measured.** No adjective stands in for a number, in the interface or the marketing.
4. **Narrow beats broad.** Fewer checks, each fast and correct, over parity with a competitor's feature list.
5. **Severity is the interface's primary signal.** What needs attention must read at a glance, in a table of a million rows.

## Accessibility & Inclusion

No external standard has been mandated. One product-specific requirement is binding: **severity must never be encoded by colour alone** — always paired with an icon and a text label. This follows from the interface being severity-dominated and from the brand accent deliberately avoiding the semantic colour band.

## Open Decisions

- **Closed-source and proprietary.** `LICENSE` is all-rights-reserved; the repository is private. Open-sourcing later remains possible and is not foreclosed.
- Whether a paid tier ever exists is deliberately not decided. No "free forever" promise may be published while this is open.
- Distribution is undecided. Nothing is published to crates.io or any package manager.
