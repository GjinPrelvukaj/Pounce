# Continuation prompt — paste this to start the next session

Continuing work on Pounce, a closed-source Rust + Tauri technical-SEO crawler.

**Read first, in this order.** Do not start work until you have.

- `CLAUDE.md` — invariants, conventions, gotchas already paid for
- `PLAN.md` — every milestone, task and gate; the first unchecked `- [ ]` is
  the next task unless the priorities below say otherwise
- `PRODUCT.md` — users, positioning, brand. **§ Users changed on 2026-08-25**
- `docs/2026-08-25-ux-debt.md` — nine specific interface failures, from the
  owner using the app on a real site
- `docs/benchmarks/` — every measurement taken so far, newest last

**Check `git status` before anything else.** Uncommitted changes are someone's
work in progress, not scratch.

## Where things stand

M0–M3 are closed. M4 is in progress: T4.1–T4.9 are done (Tauri shell, design
tokens with three theme states, the command layer, 10 Hz progress, new-crawl
screen with limits and politeness, live dashboard, pause/resume/cancel, native
dialogs and recents, and a virtualised grid measured at 500k rows — steady
scrolling drops 0.0% of frames, memory flat at ~100 MB).

**What changed on 2026-08-25, and why it reorders the work.** The owner ran the
app against his own site and could not follow parts of it. PRODUCT.md § Users
now names *marketing agency staff* as primary alongside technical SEO
specialists, and Principle 2 became "density over hand-holding, but never
rawness over meaning". The interface is currently developer-y: rule ids where
sentences belong, engine diagnostics in the header, everything at 11px, and
issue counts that cannot be clicked. Density is not the problem and must not be
traded away.

## Priorities, ahead of PLAN.md's numeric order

1. **T4.13 — issue counts become links.** Clicking `noindex · 3,913` filters the
   grid to those pages. The owner's exact complaint: *"it shows issues and how
   much. But I don't know which pages does it."*
2. **T4.21 — results while the crawl runs.** The store is disk-backed from row
   one and a WAL reader under a live writer already works (T3.3); the app just
   does not open the file until the crawl ends.
3. **T4.16 — findings read as sentences**, using the `description` and
   `remediation` the registry has carried since M2.
4. **T4.11 — sort and filter UI** bound to `SortSpec`/`FilterSpec`.
5. **T4.23 and the rest of the Craft group** — type scale, interaction states,
   empty/loading/error states, motion, alignment.

Then T4.22 (layout: issue rail with live counts, tabs, detail pane — Screaming
Frog's *arrangement*, modern components), and the remaining M4 tasks.

## How to work here

- Verify by looking. Screen recording is granted to the claude-code helper, so
  `screencapture -x -o -l <window-id>` works; get the id with
  `swift /tmp/winlist.swift` (see CLAUDE.md — the window is often behind, and a
  full-screen grab catches the wrong thing). Several real bugs this week were
  found in a screenshot after the tests passed.
- Run the app: `npm --prefix ui run dev` in one terminal, `cargo run -p
  pounce-app` in another. `cargo build` always loads `devUrl`, even in release —
  only a build through the Tauri CLI embeds `ui/dist`.
- Measure before claiming. A rAF delta is one frame interval, not a latency;
  grid numbers from a debug build are not numbers.
- Commit per task with the reasoning, tick `PLAN.md`, keep `cargo fmt`, clippy
  and the full test suite green.
