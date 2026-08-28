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

**And then the answer.** The parts summed to 0.60 s against a measured 1.53 s,
which said the missing cost was in the *pipeline* rather than in any component —
the batched writer is the one stage everything funnels through, so work added
inside it costs wall time roughly 1:1 and stalls the stages behind it. That is
testable without touching the crawl: **slow the fetch stage down and see whether
the cost survives.**

At 20k pages, twice each:

| Fixture answers | Empty registry | Full ruleset | Difference | Share |
| --- | ---: | ---: | ---: | ---: |
| instantly | 3.164 s, 3.183 s | 3.367 s, 3.414 s | **+0.22 s** | **6.9%** |
| after 5 ms | 100.640 s, 100.636 s | 100.861 s, 100.868 s | **+0.22 s** | **0.22%** |

**The absolute cost is identical to two decimal places. Only the denominator
moved.** Thirty audit rules do a fixed **~11 µs of work per page**, and whether
that is 6.9% of a crawl or two parts in a thousand depends entirely on whether
anything else is waiting.

**So Gate M2's budget is being measured against the most hostile denominator
that exists** — a localhost fixture with no latency, where the crawler is bound
by its own writer. That is the right benchmark for *throughput*, which is why
the fixture exists, and the wrong one for "what do the rules cost a user".
Against a site with 5 ms of latency, thirty rules cost two parts in a thousand.
`PLAN.md` and `CLAUDE.md` now say to quote the per-page cost and both shares,
because quoting one percentage means quoting the fixture.

**The caution stands anyway: a sum of component benchmarks is not a system
measurement.** Five instruments each said "this part is cheap" and the assembled
system was two and a half times their sum, because none of them contained the
pipeline the parts run inside.

`PLAN.md` and `CLAUDE.md` both carry the finding beside the 5.3%, and the M2 gate
keeps its tick with an instruction to re-judge before v0.1 publishes a rules-on
benchmark.

---

# Where this leaves things

*Written to be the handoff. If you read one section of this file, read this one.*

## The tree

Clean, green, pushed. `cargo fmt --all -- --check`, `cargo clippy --workspace
--all-targets -- -D warnings`, `cargo test --workspace --lib --bins --tests --
--test-threads=1` (**580 passing**), `npm --prefix ui run build` and
`npm run check:contrast` all pass as of the last commit. No stray processes, no
temporary files, no uncommitted edits.

## What landed

**All of M4** — T4.10 through T4.27, on top of T4.1–T4.9 which were already
done. The app went from a developer's scaffold to something an agency could be
handed:

- findings read as sentences and every count opens the pages it counts;
- results appear **while the crawl runs**, not after it;
- a findings rail grouped by severity, tabs that are saved questions, a filter
  bar in words, a detail pane with both directions of the link graph;
- a real type scale, one state matrix for every control, keyboard navigation
  through the grid, loading states that do not flash, errors that carry the fix;
- CSV and JSON export of exactly the view you are looking at.

**Gate M4: three of four**, with numbers — 0.00% dropped frames at 500k rows,
272 ms cold start, both themes verified on a production build.

**M5, the parts that did not need the owner** — `README.md`, `ARCHITECTURE.md`,
the bundle configuration and icon set, and the release workflow.

**Hardening** — a data-integrity bug (migrating somebody else's database), a
missing-file bug (creating an empty crawl instead of saying the file is gone),
Escape out of the detail pane, and five new measurements: export streaming
(10.2 MB peak for a 149 MB file), the two queries the results screen lives on,
the crawl re-baselined, the rule overhead at 500k, and a first profile of both
arms.

## What needs the owner, and why I did not decide it

1. **Gate M4's last item: crawl a real site without touching a terminal.** I was
   told not to crawl the owner's site, and picking an unrelated third party to
   tick a box is not a call to make unattended. Everything it depends on is
   built and exercised against the local fixture. It needs a person, a mouse and
   a site they are happy to crawl — and it is the one thing standing between M4
   and closed.
2. **Rule overhead needs a decision, not a fix.** It is 13.2% of a crawl against
   the localhost fixture and 0.22% against a site with latency — the same fixed
   ~11 µs/page either way. Gate M2's 10% is written against the first
   denominator. Whether that budget means "against the fixture" or "against a
   user's crawl" is a call about what the gate is for.
3. **T5.6 (`CONTRIBUTING.md`) and T5.9 (Sponsors) contradict `CLAUDE.md`**,
   which forbids contributor docs and public-community furniture on a
   proprietary, all-rights-reserved project. Both left undone, with the conflict
   written next to the task. Either the licence moves or the tasks do.
4. **T5.8's landing page has two identities to choose between.**
   `docs/product-plan.html` is warm — cream, cyan, Archivo and Source Serif —
   and PRODUCT.md's Brand Commitments are cool indigo on neutral with Inter and
   JetBrains Mono, in the words "not warm". The app is built to the second.
   Building the page in the wrong one is worse than not building it.
5. **The engine's politeness default is still 4 concurrent with no delay**, so
   the new-crawl screen shows its amber "as fast as the server answers" sentence
   on a default crawl. Making Gentle the default moves `FetchConfig::default()`
   and with it the CLI and every published benchmark — a spec change.
6. **The UI has no test runner.** `MiddleTruncate`, `useDelayed` and
   `columns.ts` are real logic with no coverage. Adding vitest is a defensible
   dependency and was not added unilaterally.

## Two documents that were describing a different program

**The README's benchmark table** published 17.8 s at 100k and 133.9 s at 500k.
Both predate the thirty audit rules, so they described a crawler that audits
nothing — a build nobody can download. Re-measured on the current tree with the
rules running: **21.9 s and 175.3 s**, every URL verified, and peak RSS *down*
from 279 MB to 234 MB. Slower honest numbers beat faster ones that describe a
different program.
[`docs/benchmarks/2026-08-25-crawl-rebaseline.md`](benchmarks/2026-08-25-crawl-rebaseline.md)

**`docs/2026-08-25-next-session.md`** is the prompt that started this session and
was still listing its priorities as future work. It carries a superseded banner
pointing here.

## What I would pick up first

**In this order.**

1. **Re-judge Gate M2's rule budget as a per-page cost.** The investigation
   finished: the rules cost a fixed ~11 µs/page, which is 13.2% of a crawl
   against a zero-latency fixture and 0.22% of one against a real site. Nothing
   needs optimising; the *gate* needs re-wording, and it is a judgement about
   what the budget is for rather than a measurement. Everything is in
   `docs/benchmarks/2026-08-25-rule-overhead-at-500k.md`.
2. **The Gate M4 pass with a mouse.** Half an hour with the app on a real site:
   it will find things a self-driving harness cannot, and every bug this week
   was found by looking at the window.
3. **Then M5 proper** — signing and notarisation (T5.2) is the long-pole item
   the plan itself says to start early, and the release workflow is written and
   waiting for a tag.

## Two things about how this session worked, for whoever runs the next one

**Verification by screenshot found every real bug.** A zero-height detail pane,
a modal that never dimmed, a sticky header WebKit composited underneath its
rows, a theme that did not survive a restart, a permission dialog that looked
exactly like a blank webview. None of them failed a test. All of them were
obvious in a picture.

**Driving the UI needs a harness, and the harness has three traps**, all of them
paid for tonight: React Fast Refresh preserves state, so editing a `useState`
initialiser does nothing to a running window; `load()` resets that state after
mount, so the initialiser *and* the reset both need patching; and StrictMode
invokes effects twice, so a mount-time trigger fires twice or — if you guard it
by returning early — never. Restart the app, patch both places, guard inside
the timeout.

---

## T4.28 — the screen I forgot to look at

*Added after the fact, on the owner's "the New Crawl page still looks technical
as hell". He was right and there is no defending it.*

The friendliness pass went through the results side — rail, findings, filters,
live numbers — and closed the UX-debt list without re-reading the screen that
starts everything. It presented **eight controls of equal weight**, every label
the engine's own word (Seed URL, Max depth, Time budget, Per-host requests,
Delay per request), and **two required empty fields** before Start would light.

What changed:

- **One field and a button.** The file name is proposed from the address —
  `ritecoach.com` → `~/Documents/ritecoach-com.pounce`, counted past anything
  already there so the engine will accept it — and *shown* rather than demanded.
  A file that appears somewhere nobody was told about is a file they go looking
  for later.
- **A bare domain gets its `https://` in place**, instead of an error and an
  offer to fix it. Doing what someone meant and showing them beats refusing and
  negotiating.
- **Everything else folds** behind "More options — how fast, and how much", in
  plain words: Pace, Pages at a time, Wait between requests, After this many
  pages, After this long *in minutes*, Beyond this many clicks from the home
  page.
- **The pace sentence stays outside the fold.** My first draft folded it away
  with the controls, which quietly undid T4.19: that sentence is the one that
  would have told the owner his default crawl was about to run at seven requests
  a second against a site that allows one.

**The lesson, and it is about how I checked rather than what I built:** I
verified every task against the screen it changed, and never once opened the app
the way a new user does — at the first screen, with nothing loaded. Nine items
on a list are not a substitute for using the thing.

---

## T4.34 — the total redesign

*Owner: "I need a complete redesign for this. I dont like the current one.
Total redesign, colors, layout, tokens... Everything."*

**What changed, in one sentence:** warm instead of cool, violet instead of
Linear-indigo, an app instead of a webpage — while the Screaming Frog
arrangement (tabs → grid → right filter panel → bottom detail) stays, because
that arrangement was itself the owner's request two turns earlier.

- **Palette.** Light: warm paper (`#F6F4F0` canvas, warm white surfaces, stone
  borders). Dark: warm charcoal (`#1A1815`), not blue-black. Accent violet
  `#6A4DF4`. Severity hues re-derived per theme and re-verified.
- **The rule that survived its second redesign:** the accent stays outside the
  red–amber–green band, and severity is never colour alone. Functional, not
  aesthetic — severity dominates this interface.
- **Tokens.** Radii 8/12/16. Controls carry a hairline shadow and a pressed
  state; primary buttons take a subtle top-light gradient. `--shadow` finally
  has two real jobs. Styled scrollbars — the chrome-grey webview default was
  the one smudge tokens couldn't otherwise reach.
- **Tabs** are underline tabs now (`.tab`), one class for the view strip and
  the detail pane, replacing the folder-tab utility soup.
- **Overlay titlebar.** `titleBarStyle: Overlay` + `data-tauri-drag-region`:
  the header *is* the window chrome, traffic lights float inside it, the dead
  grey strip above the app is gone. macOS-only key, ignored elsewhere.
- **The panel grew its chart.** Every row in the right panel carries a
  proportional bar in its own severity colour — Screaming Frog pairs its
  filter list with a graph; this folds the graph into the list. Pure CSS.
- **Docs moved with the code.** PRODUCT.md § Brand Commitments rewritten
  (the old text said "Not brutalist, not warm" — the owner overruled it);
  CLAUDE.md § Design tokens updated to match.

**The trap of the night:** my first regex patch of `contrast.mjs` captured
around the `#` and dropped it from every hex, so the script was computing
luminance of garbage — and I then "fixed" four colours against garbage ratios.
The give-away was byte-identical failure numbers across two different
palettes. When a checker's output doesn't move after your fix, suspect the
checker's inputs before your fix.

**Next:** the light theme should probably become the default face in marketing
material (Screaming Frog's world is light, and every approving screenshot the
owner pasted was light). The window itself keeps following the system.

---

## T4.35–T4.38 — the `$impeccable` + `apple-design` pass

*Owner picked the skills; I had done three redesigns without invoking either.*

**The finding that mattered.** `$impeccable critique` scored the app **32/40**
with exactly one weak dimension — Aesthetic and Minimalist Design, **2/4** —
which is precisely and only what the owner had rejected three times. Everything
else scored 3s and 4s. The cause was measurable and it was never colour:

    xs->sm: 11->12px  ratio 1.091  FLAT
    sm->md: 12->13px  ratio 1.083  FLAT

Three of five type steps within 2px. **No repaint can create hierarchy when the
structure that carries hierarchy does not exist**, which is why warm-violet felt
no better than cool-indigo. Two redesigns aimed at the wrong layer.

**What landed:** four type steps at 1.18/1.23/1.25 with per-step tracking and
leading (T4.35); neutrals re-tinted from hue 84° to the brand's 283.5°, whole
palette in OKLCH, pure whites gone, custom scrollbars reverted (T4.36); 43
em dashes out of UI copy (T4.37); a ⌘K palette for the persona red flag against
our own primary user (T4.38).

**Two process lessons worth more than the diffs:**

1. **The deterministic detector found zero anti-patterns**, verified against a
   positive control that correctly flagged three. The app was never
   AI-slop-looking. It was *flat*, which is a different failure and one only the
   heuristic review caught. Running both halves mattered.
2. **The contrast checker silently graded the wrong palette — twice.** First
   when a regex dropped the `#` from every hex, then when a Python `SyntaxError`
   meant the patch never wrote and the checker kept scoring the previous colours
   while printing "every pair clears its minimum". Both times the tell was
   output that did not move after a change that should have moved it. The
   checker now **throws** on an unparseable colour instead of scoring it.

**What I got wrong and the audit caught:** custom scrollbars (product register
bans reinventing standard affordances) shipped in T4.34, one hour before the
audit that flags them. And `.tab` replaced `.btn` in T4.34 inheriting its look
but not its `gap`, so detail-pane tab counts read "Linked from564" until a
screenshot caught it.

**Not done, deliberately:** Apple's gesture material (springs, rubber-banding,
velocity handoff, momentum projection) is most of that skill and none of it
applies to a click-and-arrow-key desktop grid. Taking its typography, materials
and restraint while ignoring its gestures was the point of loading it.

**Next:** `$impeccable polish` for the remaining P3s, then re-run
`$impeccable critique` to see whether Aesthetic moves off 2/4. `DESIGN.md` does
not exist — `$impeccable document` would generate it from the token file and
make future passes sharper.

---

## T4.40–T4.45 — the shell, Radix, Issues, and a rule that was right

**The two observations that drove it**, both from the owner comparing us with
Screaming Frog tab by tab: its **URL bar never leaves**, and it **shows the whole
application filled with zeros before you crawl anything**. We had both backwards.
The welcome screen I had built as "friendly" was the less friendly option: it
teaches nothing about the app behind it, and the first thing it teaches is that
things are hidden.

**Landed:** the always-visible shell with a persistent toolbar (T4.40); Radix for
the three things worth a dependency — resizable splitters, a menu instead of a
modal column picker, and column tooltips (T4.41); the Issues panel in report
vocabulary (T4.42); the detail pane filling its panel (T4.43); and a finding
swapping in the columns it is about (T4.44).

**The best finding of the session, and it was not a bug.** `description.duplicate`
was reported as a false positive: 212 pages flagged as sharing a meta description
while the grid showed visibly different text. Checked against the file rather
than argued about:

    4  Find top tennis coaches in Franklin, NJ. Browse profiles,
    → /tennis/new-jersey/gloucester-county/franklin
    → /tennis/new-jersey/hunterdon-county/franklin
    → /tennis/new-jersey/sussex-county/franklin
    → /tennis/new-jersey/warren-county/franklin

New Jersey has four Franklin townships in four counties. The site templates its
description on town name alone, so all four carry byte-identical text. **The rule
was right; the interface was showing a Title column while making a claim about
descriptions**, and consecutive rows belonged to different duplicate groups so
nothing looked duplicated. A correctness complaint that was really a presentation
bug, which is the most expensive kind to get wrong in either direction.

**Still open (T4.45):** duplicates are visible now but not adjacent, because the
grid sorts by URL and `meta_description` is not a `SortColumn`. Making it one
needs an index and a migration.

**Process note, third occurrence:** a python edit block whose first `assert`
fails silently drops every edit after it in the same block. It cost three
round-trips this session. Order the risky edit last, or make each edit its own
call.

**Layout note, second occurrence:** a leftover fixed size fights a new parent.
`h-[32vh] shrink-0` was correct until T4.41 gave the pane a resizable `Panel`
that owns its height, and then dragging the pane taller revealed canvas instead
of content. When a layout gains an owner for some dimension, every child
asserting that dimension is now wrong.

## T4.45 — duplicates sit next to each other (2026-08-28)

The finding said "more than one page uses this meta description" and the grid
answered with 212 rows in URL order, which is the one order that separates
duplicates: the four New Jersey Franklins live under four different counties.
T4.44 brought the evidence column forward; this brings the partners together.

`meta_description` was in `pages` already, so this is an index and an enum
variant rather than a schema change. Migration 014 creates the index —
maintained during the crawl, like `pages_title`, because a `.pounce` file the
user reopens has to have it and an existing file gets it on open rather than
never. Measured first, on the writer bench that T3.0 used: 5,000 pages, batch
500, ten samples, **221.6 ms before and 228.7 ms after**. +3.2%, which
criterion calls no change at p = 0.09.

`SortColumn::MetaDescription` is deliberately absent from `RANGE_SAFE_SORTS`:
it is the widest text column in the table and has not been measured at 1M
behind a range filter, so `supported_sorts` greys the header there instead.
One composite pair, `(has_issue, meta_description)` — the duplicate views
filter by `EXISTS`, which is already supported for every sort but `word_count`,
and a second wide TEXT index per filter kind is disk on a file the user keeps.

In the UI, `GROUP_BY` sits beside T4.44's `FOCUS`: selecting `title.duplicate`
or `description.duplicate` sets the sort as well as the columns. Verified
against a **copy** of the real crawl file in the scratchpad rather than the
file itself, since opening it migrates it. `EXPLAIN QUERY PLAN` reads
`SCAN p USING INDEX pages_meta_description` — index order, no temp B-tree —
and the screenshot has the four Franklins as rows 3–6.

Process note: the schema test's `rewind_to` table needs an undo line per
migration, and mine failed six tests at once with "index already exists" until
it got one. That is the table doing its job — it exists so a new migration
breaks in one obvious place rather than five obscure ones.

## T4.46 — a search preview (2026-08-28)

The first bucket-A item from the Screaming Frog triage, and the most
client-facing thing left: a "In search results" tab in the detail pane.

Everything it draws is already in `PageDetail` — title, meta description, URL —
so this is a component and a tab entry, no engine work. The decision worth
writing down is the truncation. The obvious implementation counts characters,
60 for a title and 155 for a description; the real thing cuts by pixel width,
which is why "Illinois" and "lllllllll" are not the same length. So the card is
600px wide at the type sizes a result uses and `line-clamp` does the trimming —
less code *and* closer to true. It is still an approximation, and the panel
says so rather than implying we know how the renderer behaves.

`noindex` and `nosnippet` are stated rather than simulated: a page that asks
not to be indexed gets a line saying so above a preview of what it would look
like if it were. A missing title or description says what a search engine does
in that case, which is the sentence an agency would otherwise have to write by
hand.

Verified against the copy of the real crawl file, homepage row: breadcrumb,
title and two-line description render inside the card, and both restore-checks
on the two temporarily-patched initial states came back clean.

## T4.47 — headings on the row, and the join that was measured out (2026-08-28)

The Headings tab is the next bucket-A item: first H1, H1 count, H2 count,
words. All of it is in the crawl file already, but in `page_detail` as JSON —
the one table the grid had never read.

The obvious implementation is a `LEFT JOIN page_detail`, and it reads like 200
rowid lookups against a primary key. **It is not.** SQLite computes the join
for every row `OFFSET` steps over as well, so the cost scales with scroll depth
rather than window size. Run through the M3 gate at 1M rows, an unfiltered sort
at offset 500,000 went from 7.7 ms to **494.1 ms** — 64x, against a 150 ms
gate, and `gate_m3` failed rather than printed. Baseline re-measured on a
stashed tree before believing it.

The adopted shape is a second statement keyed by the ids just returned:
`WHERE page_id IN (…200 ids)`. The `IN` list *is* the window, so the work is
the same whether the window came from row 0 or row 900,000. Every gate number
back where it was, worst pair 149.8 ms against 300 ms, memory unchanged.
Written up in `docs/benchmarks/2026-08-28-headings-on-the-row.md`, and CLAUDE.md's
narrow-row invariant gains its third case.

`h1_count` ships with `h1` deliberately: a page with two H1s shown as one
heading looks healthy, and the count is the half that carries the finding. The
grid greys 1 and ambers everything else. The `content` batch's FOCUS columns
move from title/words to h1/h1s/words, since those rules are about the heading.

Two things this did not do. Export still writes twelve `pages` columns and no
headings — the export is one statement over one table and that is what makes it
stream; flattening a list into a CSV cell is a separate decision. And nothing
here is sortable: sorting by H1 would mean the whole T4.45 exercise again, on a
column that lives in the other table.

## T4.48 — the site as a folder tree (2026-08-28)

Two of the triage's bucket-A items are one feature: Screaming Frog's site
structure view and its list-vs-tree toggle. `pounce-store::structure` groups
`pages.url` by its next path segment under a prefix; the UI asks for one folder
at a time and gets counts back, so the invariant is untouched — a tree built in
the browser from every URL is the grid mistake with an extra recursion.

Three things the first version got wrong and the screenshot showed:

**`/baseball` and `/baseball/` were two rows.** True, and unreadable: the same
word twice, one with a slash. Grouping on `rtrim(seg, '/')` merges them, the
folder takes the row and carries its own page's id, and the page is offered as
"This folder's own page" when the folder is opened. 12 folders instead of 24
rows that look like duplicates.

**A range, not a `LIKE`.** `url >= prefix AND url < prefix || char(0x10FFFF)`
uses the UNIQUE index on `url`; `LIKE 'prefix%'` does not, because SQLite only
takes that optimisation when the operator's case sensitivity matches the
index's and the default does not. There is a test for the sibling case —
`/blogroll` must not answer as a child of `/blog/`.

**A tree that stops counting looks finished.** Every level already open is
re-read on `refreshKey`, which Results bumps once a second while a crawl
writes. The tree is also keyed by the crawl's path, so opening a second file
does not inherit the first one's expanded folders.

Nothing is capped silently: a folder with more than 500 direct children reports
how many are not listed. The root is the seed's origin, read from the `crawl`
table rather than guessed from the shortest URL — which is the same answer on a
healthy crawl and quietly wrong on an interrupted one. A crawl that reached a
second host shows only the seed's, marked `ponytail:` in the source.

## T4.49 — a Duplicates tab, and the hreflang tab that is not built (2026-08-28)

"More than one page uses this title" and "more than one page uses this meta
description" are the same conversation with a client, and they lived in two
different tabs. The Duplicates tab puts all three duplicate rules in one panel.

The change that made it cheap: `ruleLines` filtered on the batch prefix, so a
panel could list `title.*` or `description.*` but never three rules from three
batches. It now accepts a full rule id as well — one line, and no second panel
component.

**Hreflang is on the list and is not built.** The data is in `page_detail`, and
after T4.47 the fetch-by-id path would make it easy. But no rule in the v0.1
thirty reads hreflang, and the reference crawl contains none at all — every
`page_detail.hreflang` is `[]`. It would be a tab of empty cells with nothing
to verify it against, which is the kind of feature that looks like progress in
a screenshot and is dead weight in use. Recorded in PLAN.md as declined with
the reason, to be built beside the first hreflang rule.

## T5.6 and T5.9 — cut (2026-08-28)

Owner's decision: Pounce is closed source and stays that way. Both tasks were
recorded on 2026-08-25 as contradicting CLAUDE.md § Conventions, which forbids
contributor docs and public-community furniture on an all-rights-reserved
project. The conflict is resolved in favour of the convention — the licence
does not move, and the two tasks are cut rather than deferred.

The "there will never be a paid tier" promise survives the cut. It is a
statement about the product, and it belongs in the README where it already
reads as one, not behind a donation button.

## T4.50 — a URLs tab, and the first UI logic check (2026-08-28)

Next off the Screaming Frog list. Length, parameters, and a Notes column saying
what is unusual about the address: capitals, underscores, encoded characters, a
path more than five levels deep. One column rather than four booleans, because
none of these is a defect on its own and four permanent "no"s is a column
nobody reads.

**The constraint worth writing down:** the rule registry is capped at thirty
for v0.1 and it is *full*, with a test that fails on thirty-one. So everything
left on the parity list is presentation, not rules — and these URL facts take
the title-length arrangement instead: shown where you are already looking,
amber past the threshold, with the findings panel left to the rules.

`urlNotes` is the first piece of UI logic in this project with a real check
behind it. No test runner in `ui/` and this is not the place to add one — Node
25 runs the TypeScript directly, so `ui/scripts/check-logic.mjs` imports the
function and asserts. Nine cases, and it failed **two of them on the first
run**:

- `%C3%A9` reported "capitals". Percent-escapes are uppercase hex by
  convention, so every encoded URL was flagged for capitals it does not have.
- `?ref=Twitter_x` reported capitals and underscores in a path containing
  neither — the slice ran to the end of the URL instead of stopping at the `?`.

Both were the kind of bug that would have looked plausible in a screenshot of a
tidy site and been wrong on every messy one. The file is named for the general
job, so the next pure function that needs a check — `MiddleTruncate`,
`CommandPalette.score` — lands there rather than in a new harness.

## T4.51 — three bugs from one real crawl (2026-08-28)

The owner crawled an 18-page site they built and found three things wrong. All
three are the same class: **the interface asserting something the data does not
say.** None would have shown up on the fixture site, and two of them are wrong
on almost every real site.

**The tree rooted at the seed.** `myzion.com` redirects to `www.myzion.com`, so
all 18 pages are stored under `www` and the prefix range built from the seed
matched none of them — a root line with nothing under it. The seed is where the
crawl was *pointed*; the shallowest crawled URL is where it *landed*. Rooting
in the data fixes every site with a canonical host redirect, which is most of
them. The fallback for a crawl with no pages is still the seed, because there
is nothing else to say.

**The Images tab was empty over 88 images.** It filtered `pages` for
`kind = 'image'`. Migration 011 moved image checks into `resources` — rightly,
since an image fetched with `HEAD` has no body, title or id, and counting them
as pages would inflate every "pages crawled" number the benchmarks publish —
and the view was never updated. So the panel counted 24 findings about images
while the grid beside it said "no pages match these filters".

The fix made `Grid` generic over its row type rather than mapping resources
into `RowView`. Mapping was the tempting shortcut and it would have destroyed a
distinction the store carries deliberately: `content_length` is nullable
because a server that declares no length is a different report from one that
declares zero, and `RowView.size` is not nullable. The column now renders "Not
declared", which is the true answer.

**133.3% and 216.7%.** Both are a count of findings about images divided by a
count of pages. 24 oversized images on 18 pages is 133%; 15 pages with issues
plus 24 image findings over 18 pages is 216%. `IssueCount` now carries
`page_urls` alongside `urls`, so a rule whose subjects are not pages shows its
count and no share — twenty-four images is a fact, "133% of the crawl" is not.
The headline row counts pages, which is also the number clicking it produces:
it filters `pages.has_issue`, so the old number disagreed with the list it
opened.

The through-line worth keeping: every one of these was a number or a view that
had drifted from the thing it names. `count(DISTINCT page_id) ignores NULLs` is
already in the gotchas; this is the same fact seen from the other side, where
the NULLs are the majority.

## T4.52 — a consistency sweep (2026-08-28)

Asked to scan for inconsistencies rather than fix one. Scanning beat guessing:
the type scale is disciplined (four steps, 59/29/6/2 uses), the dialogs all
share one shell, and the accent-bordered filter is the Signal Rule working as
designed. Three real findings:

**A 404 was two different colours.** The status→colour conditional existed in
three places — page grid, images grid, detail pane — and the images copy called
4xx critical while the other two called it a warning. The same response code
therefore looked like two different severities depending on which tab you were
on. One `statusTone()` now, with its boundaries in `check:logic`; the copy I
wrote an hour earlier is what made the divergence obvious, which is the
argument for extracting on the second copy rather than the third.

**Findings were truncated mid-sentence.** "An image the page references is
large …" and "Every page with s…". The whole reason findings are written as
sentences from the rule registry is that an agency reader should not have to
learn rule ids — and a sentence cut at 40 characters names no image, no
threshold and no page. Both panels now wrap to two lines.

**Tree rows were shorter than grid rows.** List and Tree are two readings of
one crawl; switching between them changed the density of the page. Tree rows
now take the grid's `ROW_HEIGHT`.

## T4.53 — robots.txt and the sitemap (2026-08-28)

The first of the three MVP holes, and the one with the best value per hour.
Every technical audit opens with robots.txt and the sitemap. Pounce fetched the
first for politeness and discarded it, and never fetched the second at all — so
it could not answer the two questions an auditor asks before anything else.

Four pieces:

**robots.txt is kept.** `RobotsCache::Entry` now holds the body and status it
already had in hand. `fetched()` hands them to the report. Nothing about
crawling changed.

**Sitemaps are discovered, not guessed at.** `robotxt` already parses `Sitemap:`
lines — nothing had ever asked. `/sitemap.xml` is tried only when robots
declares none, and is recorded as `found_by = 'guess'` so the report can say
which it was.

**The `<loc>` reader is hand-written.** A sitemap is one element repeated and
the crawler needs two facts from it, so a general XML parser would read a
document model nothing questions. Nine tests; the two that earn their keep are
entity decoding (`?a=1&amp;b=2` left literal produces a URL that 404s — a
"listed but not reached" finding invented by the reader) and `<image:loc>`
exclusion (an image counted as a page is the same lie). The namespace test
found a real gap on the first run: `<sm:loc>` is a page and `<image:loc>` is
not, so the prefix has to be read rather than skipped.

**The comparison is the product.** The Sitemap tab's only column that is not a
copy of the sitemap is "In the crawl", and its panel carries the two
disagreements: listed in the sitemap and reached by no link (an orphan its
owner believes is fine), and crawled, indexable, and listed nowhere. `noindex`
pages are excluded from the second — their absence is agreement, not a finding,
and reporting them would bury the real ones.

Verified end to end against the fixture: robots discovered the sitemap, 59 URLs
stored, both disagreement counts zero on a healthy site.

Two bounds, both stated rather than silent: 50 sitemap documents per crawl (an
index of 50,000 sitemaps of 50,000 URLs is a second crawl wearing a different
name) and the protocol's own 50,000 URLs per document.

## T4.54 — the grid stopped flickering (2026-08-28)

Reported from a live crawl: the list of URLs "flickers, like re-renders".
Exactly right, and the cause was one line. `refreshKey` is bumped once a second
while a crawl writes, and the effect that handles it opened with
`windows.current.clear()`. Every visible row then had no data and rendered its
skeleton until the refetch landed — 15–20 ms of blank rows, sixty times a
minute, on the one screen someone is watching *because* it is changing.

A new query and a refresh had been the same code path. They are opposite
questions:

- a **changed query** invalidates everything, and should: row 40,000 of one
  filter has nothing to do with row 40,000 of another;
- a **refresh** is the same question with newer data, and the old rows are the
  best thing to show until the new ones arrive.

So a refresh now replaces rows in place. It also refetches only the windows on
screen and drops the rest, rather than paying a query a second to keep twelve
windows warm that nobody is looking at — they are refetched anyway the moment
they are scrolled back to. Verified with three captures during a live crawl:
full rows in all three, counters advancing between them.

The run strip in the same pass. It was two stacked rows of 20px figures, about
110px of window held for the length of a crawl, above the table the crawl is
filling. Now one wrapping line, about 38px: value first and the label after it
small and quiet, because after the first glance you know which number is which.
The status chips lost their bordered pill — an icon, a word and a count in the
severity's hue is the whole message, and the border was height spent on
decoration. Both the icon and the word stay: never colour alone.

One thing the sweep found on the way: the strip's "Stop and keep what is
crawled" button called the same `cancelCrawl()` as the toolbar's Stop. Two
controls for one action, and this one was the widest thing in the strip. It is
gone, and the reassurance moved to the toolbar button's tooltip, where the
action actually is.

## T4.55 — the audit says what it did not check (2026-08-28)

The two smaller MVP holes, and they are the same hole from two sides.

**A JavaScript site is named as one.** This build reads pages as the server
sends them; rendering is M7. On a client-rendered site that produces missing
titles, missing H1s and thin content on every page — each of which is a true
statement about the HTML and a false statement about the page. The tool does
not fail on such a site, it lies fluently, and that is a far worse first
impression than a missing feature.

The signal is one no existing rule sees, because every rule looks at one field:
the *ratio* between bytes and words. 40 kB of markup carrying 30 words is a
framework's empty containers; 40 kB carrying 900 words is a page; 900 bytes
carrying 12 words is a stub and not the signal at all. Both columns are already
on `pages`, so this cost one query and no schema. Four tests, including one
that breaks each threshold on purpose.

**"Not checked in this version" is now in the panel.** This one follows from
the panel's own design. Rules that found nothing are listed at zero on purpose
— "the check ran and found nothing" is a different statement from "there is no
such check". That convention is exactly what makes an *absent* check dangerous:
a reader scanning green zeroes concludes their hreflang is fine, and we never
looked. Five absences are now named, each with a sentence saying what the crawl
does say instead.

Worth recording as the answer to "should we keep 30 rules": the cap is sound
strategy — racing a 200-check feature list is the identified primary failure
mode — but it treats a check that makes the tool *complete for a site type* the
same as one that pads a comparison table. Disclosure separates them at a cost
of an afternoon, and removes the false all-clear without adding a single rule.

## T4.56 — search kept its tab, and sitemaps follow redirects (2026-08-28)

Two reports from the owner's own crawls.

**"When I do a search it opens the table on Custom."** A view was identified by
its filters and columns together, and the search box is a filter — so typing
made every tab stop matching, the underline went out, and the panel changed
under you for narrowing the list you were already looking at. A search is a
refinement *within* a view, not a different view; Screaming Frog puts its
search above the tabs and keeps it there while you move between them. Identity
now ignores `urlContains` on both sides.

Worth noting, because it looked like the same bug and was not: the tab also
reads "Custom view" when the *columns* differ from a view's defaults, which is
correct — a customised column set genuinely is a custom view, and that is what
the first verification screenshot was showing.

**ritecoach's sitemap was not checked.** The reported cause was neither of the
two guesses. That file is at **schema 13**: it was crawled before sitemap
support existed today, and its seed was already `www`. Re-crawling populates it.

But the question found a real bug behind it. The sitemap pass used
`fetcher.fetch`, and auto-redirect is disabled client-wide because a page's
redirect chain is data the crawler must record. A sitemap's chain is not data,
it is plumbing — and a site that redirects its apex to `www` answers 301 to
`https://example.com/sitemap.xml`. That 301 parsed as a document containing no
URLs, and the whole comparison silently reported nothing. `follow` now.

The regression test declares a redirecting sitemap in the fixture's robots.txt,
which needed `Fixture.sitemap_in_robots` — defaulted to the historical value so
every benchmark before it is still reproducible. Mutation-checked: reverting to
`fetch` fails the test.

Guessing also widened, since most sites never name their sitemap in robots.txt:
six conventional addresses, stopping at the first that answers with URLs. A
guess that misses is not recorded — a 404 at an address we invented is a fact
about our guess, not a finding about the site, and "6 sitemap files read, 5
missing" would be reporting our own guesswork back as a defect.

## PLAN.md brought up to date (2026-08-28)

Everything decided in conversation this week was living only in conversation.
Now in the plan:

- **`### Deliverables`** — T4.57 Excel, T4.58 PDF report, T4.59 Word report,
  and T4.60, which is a bug the new views introduced: `Export…` compiles the
  page grid's filters, so on the Images or Sitemap tab it silently writes the
  pages table instead of what is on screen. A wrong file with no error.
- **`### Recorded, not built`** — hreflang, HTTP headers / Security / View
  Source, an External links tab, meta keywords and pagination, each with the
  reason it is not scheduled. Also the rule-cap decision: 30 stays for v0.1,
  with disclosure instead of more rules, revisited when the SDK makes rules the
  community's problem.
- **M5 opens with a table of what is left and who it is blocked on.** Two of
  seven items need the owner, and one of those — signing — is the long pole,
  because notarisation is a multi-day surprise and everything downstream of it
  is packaging.
- **T5.10 added**: the README, ARCHITECTURE.md and the product page all predate
  the sitemap comparison, the tree, the URLs tab and the disclosure. The
  README's "what this doesn't do yet" section is now the same promise the panel
  makes, and the two must not disagree.
- **Three standing rules**, each earned this week: a number on screen must
  equal what clicking it produces; say what was not checked; and verify against
  a real crawl, not only the fixture — the fixture has no host redirect, no CDN
  images and no unrendered app shell, and every one of those produced a defect
  that shipped.

The duplicated hreflang entry is gone (it was recorded twice, in two sections),
and M5's tasks read in numerical order again.

## T4.57 — the Excel workbook (2026-08-28)

The first of the three deliverables. CSV is a transfer format; a workbook is a
document, and the difference is not decoration — a header that stays put, a
filter on every column, and numbers that are numbers are what let someone sort
by word count or pivot by status without cleaning the file first.

**One sheet per question, not one per view.** The plan said one sheet per view,
which on inspection is the same pages seven times with different columns — a
file nobody can open on a real crawl. The sheets are the things that are
actually different: Summary, Issues, Pages (the view you exported, filters and
all), Images, Sitemap. Sheets with nothing in them are omitted; five sheets
with four blank reads as a broken export.

`constant_memory` mode, one row alive at a time. The invariant that keeps the
dataset out of the UI applies here too — a workbook assembled in memory would
be the single place in this product where a million rows are held at once.

Two honesty details carried through from the store. Absent is still not empty:
a missing description writes a blank cell and `content=""` writes an empty
string, which Excel tells apart with `ISBLANK`. And an image whose server
declared no length gets a blank rather than a zero, the same distinction the
grid draws as "Not declared".

Excel's own ceiling is 1,048,575 rows and a crawl need not be smaller, so the
export **says** what did not fit rather than ending mid-list and looking
complete. The count returned is the count written; the shortfall is a separate
sentence.

Generating one from the real 18-page crawl found a bug in my own summary sheet:
with no sitemap read at all it reported "Crawled and indexable, missing from
the sitemap: 18". True of the arithmetic, false of the site — every page is
missing from a file that was never found. Suppressed now, in the workbook and
in the panel, which had the same line. That is the third time this week the
same shape has appeared: a count measured against a population it was not drawn
from.

Worth noting for whoever verifies the next one: `qlmanage -t` renders an xlsx
thumbnail without Excel installed, which is how the summary sheet was eyeballed
— but it cropped the number column and made the file look empty. The sheet XML
is the source of truth.

## T4.58 — the PDF report (2026-08-28)

The second deliverable, and the one aimed at someone who will never open the
app. Not a table: the workbook is the data and this is the argument — what was
crawled, what is wrong in order of how much it matters, what to do about each
thing, and what was not examined at all.

**Authored as HTML.** printpdf 0.12 carries a real CSS layout engine behind an
`html` feature, and the alternative — placing text at coordinates — means
owning line breaking, which means font metrics. A spike settled it in ten
minutes: headings, colours, flex rows and wrapping all came out right.

Two findings from the spike worth keeping:

- **HTML entities beyond the five XML ones are not decoded** — `&middot;`
  printed literally. So the markup uses real characters, and `esc()` handles
  only `&`, `<`, `>`, which are the three that break an XML parser when they
  arrive inside a title or a query string.
- **No font needs embedding.** With an empty font map the bridge falls back to
  the PDF built-in Helvetica. Zero bytes, present in every reader, identical on
  every platform. Marked `ponytail:` with the upgrade path — embedding Inter
  means vendoring a ~300 kB TTF and carrying its OFL notice.

The draft rendered from the real 18-page crawl exposed the substantive bug:
`issue_overview.by_rule` is ordered by **count**, so the report opened on an
Opportunity affecting 24 images, above an Issue affecting 5 pages. A document
that calls itself worst-first and sorts by volume is worse than one that makes
no claim. Sorted by severity, then reach, then rule id.

The report also states a JavaScript-built site at the top rather than in a
footnote, because it changes how every number underneath it should be read.
