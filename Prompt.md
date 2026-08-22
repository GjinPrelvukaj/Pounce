# Session prompt

Paste the block below to start any session on Pounce, at any milestone. It
deliberately contains no statement of where the work has reached — that is
discovered from `PLAN.md` and `git log`, which cannot go stale the way a
sentence in a prompt does.

---

Continuing work on Pounce, a closed-source Rust + Tauri technical-SEO crawler.

**Read first, in this order.** Do not start work until you have.

- `CLAUDE.md` — invariants, conventions, gotchas already paid for
- `PLAN.md` — 11 milestones, every task, every gate
- `PRODUCT.md` — users, positioning, binding brand identity
- `docs/specs/2026-08-19-pounce-design.md` — architecture (§3 palette is
  SUPERSEDED by PRODUCT.md; everything else stands)
- `docs/benchmarks/` — every measurement taken so far, newest last
- `docs/specs/` and `docs/plans/` — the design and the task-by-task plan for
  whichever milestone is in flight. If a plan there is part-executed, finish it
  rather than improvising a parallel route through the same work.

**Work out where we are yourself.** The first unchecked `- [ ]` task in
`PLAN.md` is the next task; `git log --oneline` is what actually landed. Where
prose and checkboxes disagree, believe the checkboxes and the code, then fix
the prose. If a task looks already done, say so and move to the next rather
than redoing it.

**Skills — use these.** They are installed but may need a restart to register
in the Skill tool. If a skill is not listed, read its SKILL.md directly and
follow it.

- `ponytail` (`~/.claude/plugins/marketplaces/ponytail/skills/ponytail/SKILL.md`)
  YAGNI/minimalism for ALL coding. Stop at the first rung that holds: does this
  need to exist → already in codebase → stdlib → native feature → existing dep
  → one line → minimum that works. Shortest working diff. Never lazy about
  understanding the problem first.
- `impeccable` (`~/.claude/plugins/cache/impeccable/impeccable/<ver>/skills/impeccable/`)
  ALL UI and design work. Run `scripts/context.mjs` once per session with cwd at
  the project, then load the `reference/` doc matching the sub-command. Use
  instead of the built-in frontend-design / artifact-design skills.

**How I work on this.**

- TDD per task: write the failing tests, **run them and confirm they fail for
  the right reason**, then implement. A stub that panics is the right way to
  see red; a compile error is not. If you write the implementation first,
  revert it to a stub, get red, then restore it — the point is proving the test
  can fail, and I have shipped vacuous assertions in this repo before.
- Then `cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D
  warnings`, then the full `cargo test --workspace`, then commit.
- One commit per task. Commit messages explain **why**, not what — the tradeoff
  taken, the alternative rejected, the trap avoided.
- **Push only when I ask.** The shell does have SSH credentials and `gh`, but
  default to committing locally and telling me how many commits are waiting.
- Update `PLAN.md` in the same commit as the work: tick the task, and record
  any deviation, deferral, or newly discovered task inline under it. **Confirm
  the edit actually landed before committing** — an anchor-matched edit that
  finds no match silently does nothing, and has already shipped a commit whose
  `PLAN.md` update was missing.
- **Work in progress stays `- [ ]`.** `- [~]` means deliberately deferred with
  the reason written beside it, never "half done" — see the legend at the top of
  `PLAN.md`. Marking in-progress work `[~]` hides its remainder from the "first
  unchecked task" rule.

**Deviate when reality demands it — and say so.** The plan names specific
crates and approaches that were guesses made before the code existed. If a
named dependency is the wrong shape or costs more than it saves, use something
else and write the reason into `PLAN.md` under that task. Tell me in your reply
whenever you add, drop, or reject a dependency.

**Measurement discipline.** This project's entire positioning is speed, so a
number that is not measured is a liability.

- Benchmarks run under `--release`. Debug numbers are never published.
- Quote the command and its real output. Never let a plausible figure stand in
  for a measured one.
- Flag explicitly when something is asserted but not verified, or verified on
  only one platform. CI covers Linux, macOS and Windows. **Benchmarks are Apple
  M5 unless the doc says otherwise** — one early Windows figure survives and is
  labelled as such. A benchmark figure with no machine beside it is unusable;
  add the host or delete the number.
- When you change what a benchmark exercises, re-run it and correct any figure
  already recorded in `PLAN.md` or `docs/benchmarks/`.
- An assertion that passes without ever being exercised is worse than none.
  Check that the bound you assert can actually be reached.
- **Mutation-check anything with a boundary or a branch.** Break the code on
  purpose — move the threshold by one, invert the comparison, drop the guard —
  and confirm a test fails. Restore it. This has repeatedly caught assertions
  that were agreeing with the code rather than testing it, and twice caught a
  guard that turned out to be dead. If a mutation breaks nothing, either the
  test is vacuous or the code is.

**Do not quietly undo an invariant.** They are listed in `CLAUDE.md`; changing
one means changing the spec first, not the code. The load-bearing ones: the UI
never receives the crawl dataset, storage is disk-backed from the first row,
auto-redirect stays disabled, `pounce-bench` fixtures never depend on `rand`,
politeness defaults are correctness, v0.1 caps at ~30 audit rules, and the
repository is proprietary — no OSS headers, no contributor or community docs.

**When you finish a task, tell me:** what landed and its test count, how many
commits are unpushed, every deviation from the plan, every number you measured
versus every claim you could not verify, anything you found broken in passing,
and what the next task is.

Start with the next unchecked task in `PLAN.md`.
