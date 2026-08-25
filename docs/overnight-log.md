# Overnight log — 2026-08-25 into 2026-08-26

Append-only. One entry per landed task: what landed, what was measured, what
surprised me, and the single next thing I would do. If the session dies, this
file is the handoff.

Working rules for the session: one commit per task, `PLAN.md` ticked in the same
commit, screenshot before committing, push after each. `ritecoach.com` is never
crawled — `/Users/gjinprelvukaj/Documents/ritecoach.pounce` (3,999 pages, 4,431
issues on 3,959 URLs) is the data file for all UI work.

---

## T4.13 — issue counts became links

**Landed.** `ui/src/Issues.tsx` draws the issue overview as buttons; a click
compiles to `Filter::HasIssue` and the grid re-queries. `ui/src/severity.ts`
holds the icon/word/tone per severity, so severity is never carried by colour
alone (PRODUCT.md's binding rule) and the detail pane can reuse it.

**Measured**, on the ritecoach file: selecting `indexability.noindex` takes the
grid from 3,999 rows to 3,913, and the homepage — the one page that is
indexable — drops out. The filter/sort pair is served: `HasIssue` is an
`Exists` shape and is supported with every sort but `word_count`.

**Two decisions worth knowing:**

- The chip shows `urls`, not `issues`. `description.duplicate` has 212 of each
  here so it does not show, but for a rule that can fire twice on one page the
  two differ — and the number on a chip must be the number of rows the click
  produces, or the count lies about its own link.
- "All issues" is its own chip rather than a cleared state, because
  `HasIssue(None)` is a real engine filter (`pages.has_issue = 1`, the
  denormalised cache) and is the fastest way to see every affected page.

**Surprise, and a note for the next session's verification:** React Fast Refresh
preserves component state, so temporarily editing a `useState` initialiser to
drive a control does *nothing* to a running window — the HMR log said "hmr
update" and the screenshot was identical. The app process has to be restarted
for that trick to work. Two screenshots were wasted on this.

**Next:** T4.21 — open the store while the crawl is still writing, so the grid
fills during the crawl instead of after it.
