# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Current state

**There is no code yet.** The repository contains only documentation. Phase 0 has not been started.

Read these before doing anything substantive:

- `PLAN.md` — the master plan: every milestone, task, and gate, from empty repo to v1.0. Start here to know what to work on.
- `docs/specs/2026-08-19-pounce-design.md` — the approved design. Authoritative for architecture, scope, and branding.
- `docs/plans/2026-08-19-phase-0-benchmark-harness.md` — the Phase 0 implementation plan, 13 TDD tasks with complete code. Execute it task-by-task; do not improvise around it.
- `docs/product-plan.html` — the same design as a presentation page. Also published as an Artifact; edit this file and republish to update it in place.

## What Pounce is

A free, open-source, natively compiled technical-SEO crawler — a Rust + Tauri desktop app with a CLI, competing against Screaming Frog (Java), FreeCrawl (Electron), and LibreCrawl (Python).

**The entire positioning is speed.** The category already has free, cross-platform, unlimited-URL competitors; the only remaining differentiator is that none of them is natively compiled. Every design tradeoff resolves toward performance, and every performance claim ships with a reproducible benchmark.

This has a direct consequence for how you work here: **a performance regression is a broken build, not a cleanup task.**

## Commands

The workspace does not exist until Phase 0 Task 1. Once it does:

```bash
cargo build                                    # whole workspace
cargo test --workspace --all-targets           # all tests
cargo test -p pounce-bench --test graph_properties   # one test file
cargo test -p pounce-bench rng                 # one module's inline tests
cargo test -p pounce-bench -- --test-threads=1 # metrics tests need this (they spawn processes)
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo bench -p pounce-bench                    # criterion benchmarks
cargo bench -p pounce-bench -- --test          # compile-and-run benches without sampling
```

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
pounce-parse     lol_html streaming extraction → PageRecord
pounce-store     SQLite schema, batched writer, query API, resume state
pounce-audit     rule registry; each check is one testable unit
pounce-export    CSV / JSON / XLSX / sitemap XML
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

**Politeness defaults are correctness, not configuration.** robots.txt honoured by default, per-host concurrency caps, `Retry-After` respected, honest user-agent carrying a project URL. Aggressive settings are opt-in and clearly labelled. A fast crawler that gets its users IP-banned is a liability.

**v0.1 caps at ~30 audit rules.** Feature parity with FreeCrawl's 200+ checks is explicitly not the goal. Racing a feature list we are behind on is the identified primary failure mode. New rules land after the rule SDK exists, and then they are the community's job.

**Every audit rule ships with two tests** — one fixture that triggers it, one that does not. This is what stops rule count becoming rule debt.

**Benchmarks report failures.** Timed-out and errored runs appear in the published table rather than being dropped. A benchmark that omits the cases where a competitor struggles is an advertisement.

## Design tokens

The brand accent is cyan (`#2ED3DE` dark / `#0A7C86` light) specifically because a crawler UI is dominated by severity states. **The brand colour must never move into the red–amber–green band**, and severity must never be encoded by colour alone — always pair with an icon and a label. Full palette and type scale in the spec, §3.

## Gotchas

- **axum 0.8 uses `{param}`, not `:param`.** The 0.7 colon syntax panics at router construction.
- **`pounce` and `pounce-cli` are taken on crates.io** (an unrelated chess engine). All other `pounce-*` names are free. The CLI can publish as `pounce-seo` while installing a binary named `pounce`.
- **sysinfo's `refresh_processes_specifics` signature changes between versions**, and `Process::memory()` returns bytes (not KB) since 0.30.
- Crates stay `publish = false` until there is something worth releasing.

## Conventions

- Conventional commits (`feat(bench):`, `docs:`, `ci:`).
- **Proprietary and closed-source.** `LICENSE` is all-rights-reserved. Crates set `publish = false` and carry no `license` field — nothing goes to crates.io. Do not add open-source licence headers, contributor docs, or public-community furniture.
- Plans live in `docs/plans/`, specs in `docs/specs/`, both dated `YYYY-MM-DD-`.
