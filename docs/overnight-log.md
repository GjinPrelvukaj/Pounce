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

---

## T4.23 — the type scale, actually used

**Landed.** `text-xs` (11px) went from 44 uses to 6. The scale now has a job per
step, written into `index.css` beside it: 11px dense metadata, 12px secondary
labels and controls, 13px body and grid rows, 15px section headings, 18px the
wordmark and the live crawl numbers. `--text-lg` moved 14px → 15px and
`--text-xl` is new. Rows are 34px, and controls gained a little vertical padding
to match. `npm run check:contrast` still passes every tier in both themes.

**A real bug, found by looking at the window rather than by a test:** nothing
applied the stored theme at startup. The toggle wrote `data-theme` when clicked
and `localStorage` remembered the choice, but on the next launch the attribute
was never set again — so an explicit "Dark" on a light Mac came back light with
the toggle still reading Dark. `main.tsx` now calls `apply(storedChoice())`
before the first render, which is also early enough that there is no flash.

**What the bigger type makes obvious:** the new-crawl form now eats roughly half
the window, leaving the grid six rows on a 1600px-tall display. That is not a
regression from this task — it is T4.18 and T4.22 becoming urgent. Setup,
running and results are three states of one task and they are still stacked on
one page.

**Next:** T4.24 — states and keyboard. Hover, `:focus-visible`, active,
selected, disabled on everything interactive; a selected grid row; arrow keys to
move and Enter to open the detail pane (which T4.12 then has to exist to open).

---

## T4.12 — the detail pane (taken before T4.24, deliberately)

**Order change, with the reason:** the starting order put T4.24 (states and
keyboard, including "Enter opens the detail pane") before T4.12 (the detail
pane). Enter cannot open a pane that does not exist, and shipping keyboard
navigation whose Enter key does nothing is the "adequate, not good" outcome the
brief rules out. T4.12 landed first; T4.24 is next and now has something to
open.

**Landed.** `Store::page_detail(id)` in a new `pounce-store::detail` module: the
opposite end of the design from `query_rows`. That one reads nine narrow columns
from a million rows; this reads every column from exactly one, plus both
directions of the link graph and the findings against it — which is precisely
why migration 012 put the six repeating JSON fields in `page_detail`.

**The cap is the invariant, applied to a page.** A hub on a real site has tens
of thousands of inlinks. The lists are capped at 100 each and the counts are
queried separately, so the pane says "showing the first 100 of 4,312" rather
than either lying or shipping the graph. A test holds it.

**The JSON columns cross as JSON**, not as strings holding JSON: the engine
parses them on the way out, so the UI reads `h1[0]` with a type behind it
instead of calling `JSON.parse` five times per page.

**A layout trap worth remembering:** the pane first rendered at *zero height*
and looked like a failed command. It was `min-h-0` in a column where the grid is
`flex-1` — the grid takes every pixel the pane does not insist on, and `min-h-0`
is an invitation to take them all. A definite `h-[38vh]` plus `shrink-0` fixed
it. The same trap is waiting for T4.22.

**And a verification trap, twice now:** `load()` resets the pane's state when a
file opens, so patching a `useState` initialiser to drive a control does nothing
if the CLI handed the app a file — the reset runs after the mount. Both the
initialiser *and* the reset in `load()` have to be patched. This cost four
screenshots across two tasks; it is written here so it costs none in the next.

**Measured** on ritecoach: the FAQ page shows one finding — "The title is short
enough that it is probably not describing the page." with `16 characters` and
its fix — beside the full record. The homepage shows "Nothing to fix on this
page", which is correct: it is the one indexable page in the crawl.

**Next:** T4.24 — hover, focus-visible, active, selected and disabled on
everything interactive, arrow keys through the grid, Enter to open this pane.

---

## T4.24 — states, and a grid you can drive without a mouse

**Landed.** Three component classes in `index.css` — `.btn`, `.btn-primary`,
`.field` — carry rest, hover, active, `:focus-visible`, disabled and pressed.
Every control had been writing its own string of hover utilities, which meant
every control had a *different* set: some had `:focus-visible`, none had
`:active`, and disabled was an opacity with no cursor. The change deletes more
than it adds.

The grid is a `role="grid"` with a tab stop: Arrow up/down, PageUp/PageDown
(a screenful, computed from the scroller's height), Home/End, and Enter to open
the detail pane. **The keyboard cursor and the opened row are two different
states and are drawn differently** — an outline for where the keyboard is, an
accent fill for what the pane is showing. Conflating them would make every arrow
key a `page_detail` query.

**Verified by driving the real handler**, not by reasoning: a temporary effect
focused the scroller and dispatched five `ArrowDown` events and an `Enter` as
real `KeyboardEvent`s, which React's delegated listener picks up. The pane
opened on `/baseball`, five rows down. The harness was removed before commit.

**Two things the screenshots caught:**

- `aria-pressed` styling outranks `.btn-primary` on specificity, so the theme
  toggle's active button was drawing the pressed look, not the primary one. That
  is the right look for a segmented toggle — so `btn-primary` came off it and
  the state is stated once, in CSS.
- Utilities outrank the component layer. `btn border-transparent` plus
  `btn-primary` gives a transparent primary button. The transparent rest state
  is now absent on the pressed one rather than overridden by it.

**Interim, and it is marked as such:** with the pane open the column is
over-committed and the grid was squeezed to a one-pixel line — the same
`min-h-0` trap as T4.12, one level up. The grid has a `min-h-40` floor with a
comment naming T4.22 as the real fix.

**Next:** T4.22 — the layout. It is now the blocking task: the new-crawl form,
the issue list, the filter bar, the grid and the detail pane are all stacked in
one column and the column ran out. Issue rail on the left, tabs over one crawl,
detail pane under the grid.

---

## T4.22 and T4.18 — the layout, and three screens instead of one page

**Landed together, and they are one change.** The arrangement cannot be fixed
while the setup form is stacked above the results in the same scrolling column,
and the form cannot move out of that column without the *run* moving with it.

**What the window is now:**

- **Shell** (`App.tsx`) — header with the open file, New crawl / Open / Close,
  and the theme toggle. It owns the crawl lifecycle. The screen is *derived*:
  a crawl in flight is the running state, an open file is the results state.
- **Welcome** — the first-run empty state. Says what the program is for in two
  sentences, offers the two things you can do, and lists recents. Before this,
  first launch was a form with no explanation.
- **New crawl** — the form, and only the form. It hands `CrawlSettings` to the
  shell and steps aside; it used to own the `startCrawl` promise, which is
  exactly why the results could not appear until it was finished with them.
- **RunStrip** — progress plus Pause and "Stop and keep what is crawled", as a
  strip above the results rather than a screen in front of them.
- **Results** — the rail on the left, tabs and filter bar and grid on the right,
  detail pane underneath.

**The tabs are saved questions, not screens:** All pages, Broken (4xx and 5xx as
one `>= 400` range), Redirects, Not indexable. Each is a `FilterState` the
engine already serves, so switching tabs is a query. A tab that is not one of
them shows as "Custom" rather than deselecting everything.

**Measured live** against the fixture (3,000 pages, 40 ms delay): at 29.5 s the
header read 738 pages, the rail read 498 pages with something to fix — climbing
— and the grid was scrolling real rows while "Crawling" ticked above it. New
crawl is disabled while a crawl runs.

**The `min-width: auto` trap, twice in one task.** A flex item's minimum width
is its *content*, so a `w-80` rail full of long sentences quietly became a 32rem
rail, and inside it a `w-full` button grew past its own container and pushed the
count off the end. `min-w-0` in both places. This is the third variant of the
same flexbox trap tonight (the first two were heights) — worth remembering as a
family rather than as three incidents.

**Also fixed while here:** the grid's header row lived outside the scroller, so
horizontal scrolling would have desynced it from the columns. It is now sticky
inside the scroller and the two scroll together.

**Next:** T4.27 — column widths. With the rail taking 320px the grid's 1,240px
of fixed columns no longer fit, so Title is off-screen at this window size. That
is the task that was always going to follow the type scale, and it is now
visibly needed. T4.25 (loading skeletons, filter-matched-nothing) and T4.17,
T4.19, T4.20 remain.

---

## T4.27 — widths, alignment, and URLs that lose their middle

**Landed.** Columns are CSS grid tracks rather than pixel widths: `4.5rem` for
status and depth, `5.5rem`/`6rem` for words and bytes, `minmax(18rem, 3fr)` for
URL and `minmax(12rem, 2fr)` for title. The 1,240px of fixed columns beside a
320px rail no longer forces a horizontal scrollbar, and Title is on screen
again. Numbers are right-aligned and their headers moved with them.

**URLs truncate from the middle, with no measurement.** `text-overflow:
ellipsis` throws away `/atlantic-county/absecon` — the only part that
distinguishes a row from its thousand siblings — and keeps
`https://www.ritecoach.com/baseball/new-jersey/`, which every row shares.
`MiddleTruncate` is a truncating head box beside a `shrink-0` tail box, so
flexbox does the arithmetic at layout time: no `ResizeObserver`, no character
width constant to be wrong about, and correct through a window resize. It splits
at the last `/` when that segment fits, so the tail is a whole path segment.

**Gap worth naming:** the UI has no test runner, so `MiddleTruncate`'s split
logic is verified by looking at it. Everything else in `ui/` is layout, which a
screenshot checks better than an assertion would, but this one is arithmetic.
Adding vitest for it is a defensible dependency and is *not* done here — flagged
for the owner rather than decided unilaterally at 4am.

**Next:** T4.25 — the states an app spends time in. Loading skeletons that do
not flash, a filter that matches nothing (the message exists but the grid still
draws its header over nothing), and errors that name the next step. Then T4.17,
T4.19, T4.20, T4.10, T4.26, then export (T4.14, T4.15) and Gate M4.

---

## T4.25 — the states between the happy paths

**Landed.** `useDelayed(active, ms = 150)` gates every placeholder in the app:
the grid's skeleton, the rail's counting skeleton, the detail pane's. A query
that answers in 12 ms now shows *nothing* rather than a grey shape that arrives
and leaves before it can be read — which is a glitch, not progress. 150 ms is
chosen against M3's measured numbers: the typical filter/sort pair is 11–15 ms
at 1M and the worst supported one is 220 ms, so the fast ones stay silent and
the slow one gets a placeholder.

**The real bug underneath it:** the grid used `total === 0` to mean "nothing
matched", but that is also its value before the first count comes back. Every
keystroke in find-in-URL flashed "No pages match these filters" before the
answer arrived. `counted` now separates the two.

**The empty state carries the way out of itself** — "Clear the filters" beside
the message. An empty grid with no exit is the state people close an app in.

**Verified**: a filter matching nothing shows the message, the button, and the
"Custom" tab; skeletons were screenshotted with the delay forced to zero, since
under real conditions on this file they correctly never appear.

**Next:** T4.17 (the header is already product-shaped after T4.22 — what remains
is moving engine and rule counts to an About surface), then T4.19 politeness,
T4.20 errors, T4.10 column picker, T4.26 motion, then export and Gate M4.

---

## T4.17 — a product header, and an About surface for the diagnostics

**Landed.** `engine 0.0.1 · schema 13 · 30 rules` is out of the header — it was
a version check written for the developer, sitting where the product name is.
The header now carries the open crawl and its size, which is what someone
reading a crawl needs to know.

The diagnostics are not deleted, they are in an `ⓘ` dialog: version, "30 rules"
as *Checks*, the file format, and the open crawl's own schema — with "written by
a different build" beside it when the two disagree. That last line is the one
job the header string genuinely had.

A native `<dialog>` with `showModal()`, so the browser supplies the backdrop,
the focus trap and Escape-to-close. A hand-rolled modal gets all three wrong.

**Next:** T4.19 — politeness made legible, which is the item with a real design
question behind it: the owner ran a default crawl at 7 URL/s against a site with
a 60 requests-a-minute limit and the interface gave him no reason to expect it.

---

## T4.19 — politeness you can read

**Landed.** The Politeness fieldset gained three presets — Gentle (1 at a time,
1,000 ms), Normal (4, 0), Fast (8, 0) — and a sentence underneath that changes
as the numbers do:

- with a delay, the rate is arithmetic: `concurrency / delay` is a ceiling the
  scheduler enforces, so it says "At most 1 request a second — gentle enough for
  a rate-limited site";
- with no delay it says "4 requests at a time and no delay: as fast as the
  server answers. On a quick site that is tens of requests a second." That is
  the true answer *and* the warning, and it turns amber, adding "Many small
  sites allow about 60 requests a minute — Gentle stays under that."

Gentle is 1 request a second on purpose: 60 a minute is the limit small sites
and shared hosts most commonly enforce, and it is exactly what the owner's own
site allows — the crawl that prompted this task ran at 7 URL/s against it.

**A decision left to the owner, not taken at 4am:** the engine's default is
still 4 concurrent with no delay, so the default crawl shows the amber sentence.
Making Gentle the default would change `FetchConfig::default()` and therefore
the CLI and every benchmark, and "politeness defaults are correctness" is an
invariant in CLAUDE.md — that is a spec change, not a UI change. The interface
now states the consequence at the moment of choosing, which is what the task
asked for.

**Next:** T4.20 — errors that offer the next step. "output already exists:
/tmp/x.pounce" should offer a free name, and that wants a typed error rather
than message matching.

---

## T4.20 — errors that carry the fix

**Landed.** Two new typed `ApiError` variants, each carrying the correction
beside the complaint:

- `OutputExists { path, suggestion }` — `free_name` counts up (`site-2.pounce`,
  `site-3.pounce`, …) past whatever is already taken, so the suggestion never
  fails for the same reason twice. It counts rather than stamping a timestamp:
  `crawl-2.pounce` is a name someone would have chosen, and
  `crawl-20260825T041233.pounce` is one they have to read character by character
  to tell from its neighbour.
- `BadSeed { input, message, suggestion }` — `seed_suggestion` offers
  `https://` + input, and *only* when the corrected form actually parses, so the
  button never proposes something that fails identically.

In the UI a failure is a `{ message, fix? }` where the fix is a function over
`CrawlSettings`. The form applies it to its own fields, so each error does not
have to know which input it belongs to. "Save as ritecoach-2.pounce instead" and
"Try https://ritecoach.com" are one click.

**Typed, not matched.** The output check moved into `start_crawl` ahead of
`crawl_with`'s own `bail!`. That refusal is correct — Pounce will not write over
a crawl — and what was missing was the way out of it. Matching on the message
string would have been the other way to do it, and CLAUDE.md is explicit that
the UI branches on `kind` rather than on text.

**Verified** both paths on screen: `ritecoach.com` offers `https://ritecoach.com`
(the seed is checked before the output, so it fires first), and a valid seed with
a taken output offers `ritecoach-2.pounce`. No request left the machine — both
refusals happen before the fetcher exists.

Two unit tests: the free name counts past what is taken, and a seed that already
has a scheme gets no suggestion.

**Next:** T4.10 (column picker with persisted layout), T4.26 (motion), then
export (T4.14, T4.15) and Gate M4.

---

## T4.10 — the column picker

**Landed.** A "Columns" button opens a native `<dialog>` of checkboxes; the
choice is stored in the window's own storage beside the theme and the recents
list, because a column layout is a property of the person and not of any
`.pounce` file — a crawl handed to a colleague should not arrive carrying
someone else's preferences.

Two new columns give the picker something to do: **Type** (Page / PDF / Image /
No type) and **Indexable** (Yes / No — noindex), both off by default. The grid
is better narrow, and the filter bar already answers both questions.

**Two small rules in the loader.** Stored keys are filtered against this build's
own column list, so a key from a version that had a column this one does not is
dropped silently — the layout is a preference, not data. And an empty layout is
never stored: a grid with no columns is one you cannot get back from without
clearing storage.

**Reordering is not in it.** Toggling keeps `COLUMNS` order, so the grid never
reshuffles under the hand that ticked a box. Drag-to-reorder is a different
feature with a different interaction, and nothing has asked for it yet.

**Next:** T4.26 (motion where PRODUCT.md already allows it), then export —
T4.14 and T4.15 — and Gate M4.

---

## T4.26 — motion, and a modal that was not dimming anything

**Landed.** Two animations, both 120–150 ms and both on state:

- the detail pane rises 8 px as it arrives, because it comes from the row that
  was clicked and a pane that appears without motion is indistinguishable from
  the window redrawing;
- dialogs fade in, using `@starting-style` so there is a "before" to animate
  from. Browsers without it get no fade and are otherwise correct, which is why
  it is the last word rather than the mechanism.

**Rows are deliberately not animated.** Fading a landed window means keying
cells by row id, which changes how React reconciles a virtualised list and would
replay the animation on every scroll. T4.9 measured 0.0% dropped frames at 500k
rows; that number is worth more than a fade. I wrote it, looked at what it would
cost, and took it out again — the reason is in the stylesheet so the next person
does not re-add it.

**And a real bug, found by measuring rather than by looking.** I suspected my
own backdrop transition of leaving the dialog's backdrop stuck transparent, so I
sampled the mean pixel brightness behind an open dialog across three
screenshots. All three were identical at **0.0392** — including one taken before
the motion work existed. The backdrop had *never* dimmed: the `backdrop:`
utility on the element was not producing a rule. A plain
`dialog::backdrop { background: rgb(0 0 0 / 0.5) }` takes it to **0.0314**.

The lesson is the method, not the pixel: I nearly committed a comment blaming a
WebKit transition bug for something that was never a transition at all. Two
screenshots that *look* the same in a dark theme can differ by a factor that
matters, and two that look different can be identical.

**Next:** export — T4.14 (CSV and JSON, streamed) and T4.15 (the current filtered
view, not just everything) — then Gate M4.

---

## T4.14 and T4.15 — export, streamed and filtered

**Landed.** A new `pounce-export` crate. `export(store, filters, sort, format,
out)` is one statement handed to SQLite and one `Write` handed to the caller,
with exactly one row alive between them. Collecting into a `Vec<Row>` first
would work on a test crawl and take a gigabyte on a user's — the same failure
the whole query design exists to avoid, one layer out.

**T4.15 is not a second feature.** Exporting the current view and exporting the
whole crawl are the same call with a different `FilterSpec`, so they cannot
drift; a separate "export all" would be a second query builder to keep in step
with the first. A test asserts the filtered export returns only the filtered
rows.

**Two format decisions worth keeping:**

- CSV quoting is RFC 4180 and hand-rolled — eleven lines against a dependency.
  A URL with a comma in it is not exotic (query strings have them) and an
  unquoted one silently shifts every column after it, so the test asserts the
  field *count* per row, not just the text.
- JSON keeps `null` where CSV has one empty field for both absent and empty.
  That is the distinction the store has carried from the parser, and the export
  is where it either survives or does not. Documented in the module rather than
  papered over: anyone who needs to tell a missing `<title>` from
  `<title></title>` wants the JSON.

**Measured** through the app on the ritecoach file: 3,999 rows to CSV (1.4 MB)
and to JSON (2.0 MB), with the row count reported beside the button — a file
written silently is a file the user goes looking for.

**Not measured, and worth doing:** peak RSS during a 500k-row export. Streaming
is true by construction here (one row, one `BufWriter`), but "never materialises
in memory" is the kind of claim this project measures rather than asserts. It
needs a seeded 500k store; if there is time tonight it goes in the hardening
pass, and if not it is the first thing to measure before the claim appears in
any user-facing copy.

**Next:** Gate M4 — crawl a real site start to finish without touching a
terminal, the table at 500k, cold start under 400 ms, both themes.

---

## Gate M4 — three of four closed, with numbers

**Written up in [`docs/benchmarks/2026-08-25-gate-m4.md`](benchmarks/2026-08-25-gate-m4.md).**
All measured on a production build through the Tauri CLI, because `cargo run`
always loads `devUrl` and would have measured Vite.

- **500k rows: 0.00% dropped frames.** 360 frames of continuous scrolling,
  baseline 17.0 ms, worst 19.0 ms. Peak RSS 79–116 MB against a 681 MB file.
- **Cold start: 272 ms median** of thirteen launches, every warm one under
  300 ms. The 470 ms outlier is the first launch after a build.
- **Both themes** captured from the production window.
- **Crawl a real site without a terminal: still open**, deliberately. This
  session was told not to crawl the owner's site, and picking an unrelated
  third-party site to tick a box is not a call to make unattended.

**Two real grid defects came out of the 500k screenshots**, and both are the
kind only looking finds:

1. The sticky header lived *inside* the scroller. The transformed rows get their
   own compositing layer and WebKit painted a sliver of one **over** the
   header's top edge — hit-testing landed on the header while a row was visibly
   drawn across it. Neither `z-index` nor `transform-gpu` fixed it, because it
   is compositing order rather than paint order. The header is a sibling above
   the scroller again; T4.27's elastic tracks had already removed the
   horizontal scrolling that put it inside.
2. Header cells had no `pr-3` while their data does, so every right-aligned
   heading sat twelve pixels right of the numbers it labelled.

**The diagnosis is worth keeping more than the fix.** I first blamed the
virtualiser and added a `scrollMargin`; the geometry probe then showed
`header.top === scroller.top` exactly, and `elementFromPoint` at the scroller's
top edge returned the header's own sort button. The DOM was right and the pixels
were wrong, which is the signature of a compositing problem — and the way to see
it was to ask the page where things *are*, not to reason about the CSS.

**Next:** M5 work that does not need the owner. `PLAN.md`'s M5 is the MVP
release; the parts that can be done unattended are the ones that do not require
signing, publishing or a decision. After that: hardening — the export RSS
measurement at 500k that T4.14 left unmeasured is first.

---

## Export streaming, measured — and the M5 docs

**The claim T4.14 left unmeasured is now measured**, in
[`docs/benchmarks/2026-08-25-export-streaming.md`](benchmarks/2026-08-25-export-streaming.md):
500,000 rows out of a 681 MB store is 77 MB of CSV in 1.34 s and 149 MB of JSON
in 1.54 s, both at **10.2 MB peak RSS**. Identical peaks across two formats whose
outputs differ 2× is the shape of a stream; had the rows accumulated, the peak
would have tracked the output. The ignored test takes its paths from the
environment and runs as a bare binary under `/usr/bin/time -l`, because through
`cargo test` you measure cargo.

**T5.5 `ARCHITECTURE.md`** leads with query-don't-dump and derives the rest from
it, with the measured number beside each decision rather than the reasoning that
suggested it.

**T5.4 `README.md`** puts the scale table above the fold and states the gaps.
The FreeCrawl head-to-head is present but explicitly not the headline: it is
stale *in our favour*, and a stale ratio in your own favour is exactly the kind
this project re-takes rather than repeats.

**A conflict for the owner, not for me.** T5.6 (`CONTRIBUTING.md`), T5.9 (GitHub
Sponsors) and T5.4's "dual-licence note" all predate the move to proprietary and
all-rights-reserved, which `CLAUDE.md` now states as a convention forbidding
contributor docs and public-community furniture. I wrote the README to match the
actual `LICENSE` and left T5.6 and T5.9 undone, with the conflict recorded next
to the task in `PLAN.md`. Whether the licence moves or the tasks do is a decision
about what this product *is*.

**Next:** T5.1 (bundler config) is the remaining M5 item that does not need the
owner — it can be written and a `.dmg` built locally, though signing (T5.2) and
the release matrix (T5.3) both need credentials and a tag. After that, hardening.

---

## T5.1 — bundling, and what macOS does to a bundle

**Landed.** Bundle targets, identifier, category, publisher, copyright, licence
file, macOS 10.15 minimum, per-machine NSIS, Debian dependencies. The mark is
redrawn at 1024 (the generator is committed beside it, so the icon is
reproducible rather than a binary someone has to re-trace) and the full icon set
generated from it. The iOS and Android sets `tauri icon` produces were deleted:
this is a desktop app, and an icon directory implying otherwise is a lie about
the target.

**The macOS `.app` builds and runs** — 17 MB, 500,000 rows open and scrolling.

**Two things this machine could not close.** The `.dmg` step shells out to
AppleScript for the disk-image window layout and needs Finder scripting
permission. Windows and Linux bundles need those platforms — that is T5.3.

**The finding worth more than the config.** A bundled app is subject to macOS
TCC; a bare binary run from a terminal inherits the terminal's permissions. The
first launch of `Pounce.app` against a `.pounce` file in `~/Documents` raised
*"Pounce would like to access files in your Documents folder"* and, while that
sat unanswered behind another window, the app painted its background and nothing
else — **indistinguishable from the blank-webview failure this project has
already been bitten by.** The same file under `/tmp` opened instantly. It is in
`CLAUDE.md` now, with the distinguishing test: screenshot the whole screen, not
the window.

I did not answer the prompt — granting a permission on the owner's behalf is not
mine to do — and dismissed it with `killall UserNotificationCenter`, which
neither grants nor denies. The screen was verified clean afterwards.

---

## Hardening: a 23% regression that was not one

**Re-baselined the crawl**, because `docs/benchmarks/` last measured one on
2026-08-21 — before the thirty rules and migrations 012 and 013 — and "a
performance regression is a broken build" means nothing if nobody re-measures.

10k came back at 1.6 s and 25 MB, unchanged to the tenth of a second. **100k
came back at 21.8–22.5 s against a published 17.8 s.**

The rules account for about a second of the four. The rest I chased instead of
assuming: re-running the end-to-end A/B showed **both arms had moved, including
the one that runs no rules at all**. The suspect was T4.4's move to
`run_controlled_pipeline`, so I built the commit immediately before it in a
worktree and ran the identical test in the same session: 20.239 s against
HEAD's 20.226 s. The old tree is exactly as slow. There is no regression — the
laptop is about 8% slower than it was on 2026-08-23, after five hours of
continuous release builds.

Written up in
[`docs/benchmarks/2026-08-25-no-regression-recheck.md`](benchmarks/2026-08-25-no-regression-recheck.md).
Two things came out of it worth keeping:

- **Rule overhead is environment-sensitive**: 5.28% idle, 6.13–7.61% loaded,
  against Gate M2's 10% budget. The gate passes with less headroom than the
  published number implies, and `CLAUDE.md` now says so. A future rule batch
  should be measured on a quiet machine before anyone calls it free.
- **Cross-day comparisons on a working laptop are worth about ±10%.** Building
  the old commit beside the new one in the same session costs a worktree and
  four minutes, and it is the entire difference between "no regression" and
  "23% slower, cause unknown".

**Deliberately not done: re-taking the FreeCrawl head-to-head.** It needs
cloning a third-party repo and running its `npm install` on the owner's machine,
which executes that project's lifecycle scripts. That is a supply-chain decision
to make awake and with intent, not at 5am unattended. The README already states
that the ratio is stale and is not quoted as a headline.

---

## T5.3, and a hardening pass on the doors into the app

**T5.3 release matrix** written (`.github/workflows/release.yml`): four native
bundle jobs on tag, artifacts collected per platform, attached to a **draft**.
Two macOS jobs rather than a universal binary, because cross-compiling the Intel
half works until a dependency with a C build disagrees and `rusqlite`'s bundled
SQLite is one. Linux on ubuntu-22.04 so the AppImage's glibc is old enough to
start elsewhere. Written, never executed — it needs a tag, and until T5.2 the
installers are unsigned.

**A real data-integrity bug, found by asking what happens at the edges.**
`Store::open` created the file when missing and migrated whatever it found
otherwise. Drag the wrong file onto the window — a Core Data store, an app
cache, anything — and eleven `CREATE TABLE`s went into somebody's database,
silently, with no undo, after which the app opened "successfully" and showed an
empty crawl. Two guards now: a file at `user_version = 0` that already has
tables is refused before any migration runs, and a file claiming our schema
without our tables is refused too (`user_version` is a free integer and other
programs use it). Tests cover both, including that the foreign database is
untouched afterwards.

The refusal is typed, so the window says *"invoices.sqlite is a database, but
not a Pounce crawl. Choose a .pounce file, or start a new crawl."* — and it
reaches the window through **both** doors: a `startup_error` command carries the
argv failure to the welcome screen, because "Open With" is a door people arrive
through and stderr is not somewhere they look.

**Escape closes the detail pane** and puts focus back on the grid. The pane
opened with Enter and had no keyboard way out; an interaction you can enter with
the keyboard and only leave with the mouse strands you. Verified by dispatching
real key events and reading `document.activeElement` back.

**A test that now pins a promise:** cancelling a crawl leaves a *finished* file.
The deferred index builds and the site rules live after the frontier loop, so a
cancel that returned early would leave rows without an inlink index and without
site findings. Mutation-checked — returning early before `build_link_index`
fails the test.

**Toolbar fix:** on a 1,280px window the whole filter row wrapped and put
"Columns" on a line of its own. The filters wrap inside their own box now; the
buttons do not move.

---

## The rail reads worst-first, and the last engine word is gone

**UX-debt item 4 closed**, which was the last of the nine. The live crawl said
"Queued 0 waiting" and reported response classes as `1xx`–`5xx`. The classes are
words now — Worked, Redirected, Not found, Server error — with the code in the
tooltip, because a specialist reads in codes and both people are looking at the
same crawl. "Queued" became "Still to fetch", which is the question it answers.
Fetches that got no response at all are "Never answered".

**The findings rail groups by severity, worst first.** The engine orders by
count, which put a 4,000-page notice above a two-page critical — a fair ordering
of numbers and a misleading ordering of problems. Count order is kept *inside* a
severity, where a bigger number does mean a bigger job. The headings carry no
number: rows count URLs, `bySeverity` counts findings, and two units stacked on
each other is the confusion the rail exists to remove.

The UX-debt document is now annotated with the task that closed each item and
kept as the evidence behind PRODUCT.md's amended § Users, not as a live list.

---

## Chasing the rule overhead — five candidates, and a caution

This is the largest thing in the session that is *not* a feature, and it started
from re-baselining the crawl.

**The finding:** rule execution costs **13.2% of crawl wall time at 500k**
against Gate M2's 10% budget, and the share is not flat with corpus size the way
the M2 write-up assumed (5.25% at 10k, 5.28% at 100k). The 2026-08-23 file
listed "no end-to-end A/B at 500k" as a known gap; the gap is filled and the
claim did not survive it.
[`docs/benchmarks/2026-08-25-rule-overhead-at-500k.md`](benchmarks/2026-08-25-rule-overhead-at-500k.md)

**What was ruled out, in order, each with a number:**

1. the machine (a proportional slowdown leaves a ratio unchanged; the same A/B
   at 100k moved 5.28% → 7.61% under load, worth ~2 points);
2. the site rules (848 ms at 500k, linear);
3. the deferred index build over issue rows (0.20 s at 100k);
4. the `has_issue` update, which *shrinks* as a share with scale;
5. **CPU contention** — each arm was run alone under `/usr/bin/time -l`, and
   the rules arm burns +1.6 s of CPU for +1.53 s of wall. They match, so the
   work is real and no amount of scheduling fixes it;
6. the instrument's shape — issue density, writer batch size, 28 links per page,
   and detail strings were all added to the seeded arm, moving the store-side
   figure by 0.02 s;
7. the page-rule microbenchmark being taken on an easier page — it is not; the
   audit bench renders the fixture's own pages, links and all.

**Where it ended.** Every component is measured against a fixture matching the
crawl, and the parts sum to **0.60 s against a measured 1.53 s**. Forty per cent.
None of the five instruments is wrong on its own; something about assembling
them costs 0.9 s per 100,000 pages that none of them sees.

**The caution is worth more than the number: a sum of component benchmarks is
not a system measurement.** The end-to-end A/B is the only figure that has ever
been trustworthy for this question. The next person should *profile* a crawl with
and without the registry rather than price a sixth component; elimination has
been taken as far as it goes.

`PLAN.md` and `CLAUDE.md` both carry the finding beside the 5.3%, and the M2 gate
keeps its tick with an instruction to re-judge before v0.1 publishes a rules-on
benchmark.
