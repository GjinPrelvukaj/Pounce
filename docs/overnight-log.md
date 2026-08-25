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

---

## T4.21 — results while the crawl runs

**Landed.** `Store::open_read_only` (a `SQLITE_OPEN_READ_ONLY` connection that
does *not* migrate) plus one branch in `open_crawl`: a file with a crawl in
flight opens read-only, anything else opens as before. The pane opens the
output path on the first progress tick that reports written work, and re-queries
once a second while the crawl runs.

**Measured**, against the local fixture site (3,000 pages, 40 ms delay, ~25
URL/s): at 11 s the grid was empty, at 26 s it held 500 rows and the issue chips
read 1,113 findings on 498 URLs, both climbing. Nothing was crawled off this
machine.

**The surprise, and it changed the design:** `written` in a progress tick is
pages *handed to the writer*, not pages committed. The batch is 500, so the
first ~25 seconds of a polite crawl show "353 pages so far" beside an empty
grid. That is not a bug — batching is why the write path scales — but the
interface was saying *"No issues found — every rule this build has passed on
every page"* while 353 pages sat unwritten. A clean bill of health the crawl had
not earned. Both empty states are now live-aware and name the batch.

**Two things deliberately not done:**

- The overview is not queried at 10 Hz, and only one query is allowed in flight
  at a time. `issue_overview` is a `GROUP BY` over every issue in the file; at
  500k pages that is a scan competing with the writer, and self-throttling on
  the in-flight flag costs nothing when it is fast.
- Read-only is the *guarantee*, not politeness. `Store::open` migrates, and two
  connections racing `PRAGMA user_version` on one file is the two-writers hazard
  `may_start` refuses at the front door, arrived at through the back. A test
  asserts the second connection cannot write.

**Next:** T4.16 — findings read as sentences. The chips still say
`indexability.noindex`; the registry has carried a `description` and a
`remediation` per rule since M2 and the interface shows neither. It needs a
command exposing rule metadata, since the store keeps only ids.

---

## T4.16 — findings read as sentences

**Landed.** A `rules` command hands the registry's `RuleMeta` across once per
session, and the issue list renders `description` instead of the rule id.
Selecting a rule shows its `remediation` beneath the list — the other half of a
finding, since a rule that says what is wrong and not what to do about it is a
complaint. The id survives as the row's `title` attribute and in the metadata
line, because `--fail-on` and exported reports address rules by id forever.

**Measured** on ritecoach: `indexability.canonical-elsewhere · 6,000` — the
example in the UX-debt doc — is gone. The list now reads "The page tells search
engines not to index it. 3,913" with "Fix: Remove the noindex directive if this
page should appear in results. If it should not, no action is needed."

**Why the prose is not in the store:** issue rows carry a `&'static str` rule
id, which costs nothing per row and cannot go stale. Denormalising the sentence
onto four million issue rows would be the same paragraph written four million
times, wrong the moment the wording improved. The registry is the source and the
command is the only crossing.

**A test now enforces the thing the interface assumes:** every registered rule
has a non-empty description and remediation, and the description ends in a full
stop. A rule shipped with a fragment renders as a fragment.

**Layout note for T4.22/T4.27:** at 24rem minimum column width, three columns of
findings fit a 1800px window and the longest descriptions truncate. That is the
right shape for a wide window and the wrong one for the left rail T4.22 wants —
the rail will be one column, so the truncation goes away rather than needing a
tooltip.

**Next:** T4.11 — the sort and filter UI, asking `supported_sorts` which pairs
the engine will run rather than keeping a second list in TypeScript.

---

## T4.11 — sort and filter UI

**Landed.** `ui/src/Filters.tsx` is a filter bar over `FilterSpec`: find-in-URL
(debounced 250 ms), response class, body type, indexable, and depth. Grid
headers are buttons over `SortSpec` — click to sort, click again to reverse.

**The part worth keeping:** which sorts are offered comes from the engine's
`supported_sorts`, never from a list here. It shows: with `baseball` typed into
find-in-URL, every header except **URL** greys out, because a substring filter
is a `Substring` shape and `is_supported` allows it with `SortColumn::Url` alone
— no B-tree serves a substring match, and any other sort is a table lookup per
skipped row. Clear the box and all six headers come back. A second list in
TypeScript would have offered those sorts and taken an `UnsupportedPair` error.

**Measured** on ritecoach: `2xx` + `Pages` + `baseball` = 565 of 3,999 rows.
Sorting by Bytes descending puts `/soccer` first at 121,399 bytes.

**A status *class* is two filters, not one.** The store has `status`, not
`status_class`, so `4xx` compiles to `>= 400 AND <= 499` — two ranges, which is
why choosing one narrows the sorts on offer to `RANGE_SAFE_SORTS`. That falls
out of asking the engine; nothing here had to know it.

**Fallback rule:** when a filter change makes the current sort unsupported, the
sort resets to `url`. That is the one column supported against every filter
shape this build has — a substring filter is *only* offered with it.

**Next:** T4.23 — the type scale. `text-xs` at 11px is still the whole
interface; the new filter bar and issue list joined it. Rows and body to 13px,
secondary labels to 12px, 11px for dense metadata only.
