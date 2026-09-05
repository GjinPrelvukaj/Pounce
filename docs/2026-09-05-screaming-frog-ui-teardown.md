# Screaming Frog 24.3 — UI teardown

**Source:** 39 screenshots in `~/Downloads/Screaming Frog UI Detailed`, a crawl
of `myzion.com` (118 URLs) on macOS, taken 2026-09-05. Seventeen were read in
full; the rest repeat the same shell with different columns.

**Why this exists.** The owner supplied Screaming Frog references repeatedly and
they were not used. Three redesigns changed Pounce's wording, palette and type
scale and never touched its information architecture, which is the thing the
references were about. This document is the missing analysis.

---

## 1. The frame

Nine regions, and **they never move**. Every tab, every state, every screenshot:
identical geometry. Only the grid's columns, the right panel's contents and the
chart change.

| | Region | Height | Contents |
|---|---|---|---|
| 1 | Title bar | 28px | `https-myzion-com- - Screaming Frog SEO Spider 24.3 (Unlicensed)` — the crawl name, not the app name |
| 2 | Header bar | 40px | Logo · URL field (full width, ✕ clear, ▾ history) · crawl-mode dropdown (`Subdomain`) · `▶ Start` / `⏸ Pause` · `Clear` · progress bar (`Crawl 100%`) · mode label (`SEO Spider`) |
| 3 | **Two tab strips, one row** | 22px | **Left:** 20 data tabs (`Internal External Security Response Codes URL Page Titles Meta Description Meta Keywords H1 H2 Content Images Canonicals Pagination Directives Hreflang JavaScript Links AMP ▾`). **Right:** 8 panel tabs (`Overview Issues Site Structure Segments Response Times API Spelling ▾`) |
| 4 | Grid toolbar | 28px | Filter dropdown (context-sensitive; two of them on Response Codes) · list/tree toggle · `Export` · right-aligned `Search` + settings icon |
| 5 | **Main grid** | ~48% | The data |
| 6 | Grid footer | 18px | `Selected Cells: 0    Filter Total: 19` |
| 7 | **Bottom pane** | ~30% | Own toolbar, own content, own footer |
| 8 | Bottom tab strip | 22px | 14 tabs (`URL Details Inlinks Outlinks Image Details Resources SERP Snippet Rendered Page Chrome Console Log View Source HTTP Headers Cookies Duplicate Details Structured Data Details Lighthouse Details ▾`) |
| 9 | Status bar | 20px | `Spider Mode: Idle` · `Average: 11.98 URL/s. Current: 11.98 URL/s.` · `Completed 118 of 118 (100%) 0 Remaining` |

The right column is ~30% of the width and splits into a **panel table** (~45%)
above a **chart** (~40%).

## 2. The one idea that organises everything

**Left tabs are aspects. The right panel is the index of every check. The filter
dropdown is the same index, scoped to the tab you are on.**

The right panel under `Overview` is a scrolling tree:

```
▼ Summary
    Total URLs Encountered        99   100%
    Total Internal Blocked by robots.txt   0     0%
    …
▼ Crawl Data
  ▼ Internal
      All          18   100%     ← selected, filled green
      HTML         18   100%
      JavaScript    0     0%
      CSS           0     0%
      …
  ▼ Security
      All          19   100%
      HTTP URLs     0     0%
      Mixed Content 0     0%
      Unsafe Cross-Origin Links   18  94.74%
      Missing HSTS Header          0     0%
      …
  ▼ Page Titles
      All          18   100%
      Missing       0     0%
      Duplicate     0     0%
      Over 60 Characters   2  11.11%
      Over 561 Pixels      2  11.11%
      Same as H1           4  22.22%
      …
```

Two columns: `URLs` and `% of Total`. **Every check is listed at every state,
including zero.** Clicking a row drives the left grid to that tab and filter.

That panel is how Screaming Frog is navigated. It is a table of contents, a
progress report and the filter UI in one control, and it is always on screen.

## 3. Look

- **Chrome:** macOS Java/Swing. System grey, white grid, 1px `#D8D8D8` rules,
  square corners on everything but native buttons.
- **Type:** one size, ~11px system sans, everywhere. Column headings are the
  same size and weight as the data. **There is no type scale at all.**
- **Colour:** one brand green (`#76B82A` family) doing six jobs — logo, `Start`,
  selected tab, selected row in the panel, progress bar, chart series, and the
  `Total: 28` chip. Everything else is greyscale.
- **Severity colour appears in exactly two places:** the Issues tab
  (🛑 red `Issue`, ⚠ amber `Warning`, ⓘ blue `Opportunity`, each **icon + word**)
  and the Site Structure chart legend (grey/pink/green/amber/red/dark-red for
  blocked / no-response / 2xx / 3xx / 4xx / 5xx).
- **Zero decoration.** No shadows, no gradients, no illustration, no empty-state
  art. Empty states are grey centred sentences: *"No URL selected"*, *"No data"*,
  *"Only available when JavaScript Rendering is enabled"*, *"Not configured to
  store HTTP Headers"*, *"Please configure 'Segments' to populate this table"*.
- **Charts** are plain and per-tab: a donut for composition (Internal, External,
  Response Codes), a bar chart for counts (Security, Meta Description, H1,
  Content, Directives, AMP, Response Times), with rotated axis labels.
- **Selection is cell-based, not row-based.** Selecting a URL fills only the
  Address cell green, and the footer reads `Selected Cells: 1`. It behaves like
  a spreadsheet, deliberately.

## 4. Feel

It feels like **a spreadsheet with a crawler bolted on**, and that is its
strength. A person who has used Excel or any database GUI already knows how to
drive it.

- **Unafraid of density.** Every pixel is data. Nothing is spaced for comfort.
- **Nothing is hidden and nothing is explained.** Discoverability is by
  exhaustive enumeration — 20 tabs, 14 bottom tabs, a panel listing 100+ filters
  at zero. It never teaches; it displays everything and lets you find it.
- **Utterly predictable.** The frame is identical on every screen, so you learn
  it once and never re-orient. This is the single biggest usability asset and it
  costs nothing but discipline.
- **It is not warm, not inviting, and not beautiful.** It is grey, cramped,
  1990s-desktop, and completely unembarrassed about it.
- **It is fast to scan**, which is a different quality from fast to learn, and it
  is the one the daily user actually wants.

## 5. Flow

1. **Paste URL into the header, press Start.** No dialog, no wizard, no setup
   screen, no project. One field and one button.
2. **Watch it fill.** The grid populates live; the header progress bar reads
   `Crawl 21%`, the status bar reads `Completed 19 of 90 (21.11%) 71 Remaining`
   and `Average: 7.89 URL/s`. The table never jumps.
3. **Scan the right panel.** Top to bottom, it is every check with a count and a
   percentage. This is the "what's wrong with this site" read.
4. **Click a filter row** → the left grid jumps to that tab, filtered.
5. **Click a URL row** → the bottom pane fills. Choose a bottom tab for Inlinks,
   Outlinks, Resources, the SERP preview, rendered page, source, headers.
6. **Export** — and there are **three separate Export buttons**, one per region
   (grid, bottom pane, right panel), each exporting its own scope.

Two details worth stealing outright:

- The **bottom pane has its own filter row** where it needs one. `Outlinks` gets
  `All Link Types ▾  All Link Origin Types ▾  Show Links (5/5) ▾  All Links ▾`.
- The **`SERP Snippet` tab is an editor, not a preview**: a live Google result
  with a `Chars / Pixels` table (Length, Displayed, Truncated, Available,
  Remaining) and inputs to try a different title and description against the
  561px / 985px limits.

## 6. Where Pounce already matches

The skeleton is closer than the complaint suggests. Pounce already has: a
persistent crawl toolbar in the header, one screen with no welcome state, a view
tab strip, a virtualised grid, a right panel with counts, a detail pane at the
bottom with its own tabs, and export that follows the view (T4.60 — the same
insight as SF's three Export buttons, reached independently).

**This is not a rewrite. It is a re-organisation.**

## 7. Where Pounce diverges, and is wrong to

1. **The right panel is findings-only; SF's is an index of everything.** Pounce
   shows Overview (a summary) and Issues (a findings list). Neither is the
   exhaustive, always-visible, click-to-filter list of every check at every
   state with a count and a percent. **This is the single biggest difference**
   and it is why Pounce feels like it hides things: the panel that should be the
   table of contents is instead a report.
2. **The filter dropdown is not bound to the tab.** SF has one context-sensitive
   `All ▾` (two on Response Codes) that changes meaning per tab. Pounce has five
   generic dropdowns that are identical on every tab and mostly irrelevant to
   the one you are on.
3. **The tab strip mixes aspects with findings.** SF keeps aspects in the tabs
   (`Internal`, `Page Titles`, `H1`, `Content`, `Images`, `Canonicals`,
   `Directives`) and findings in the panel. Pounce's strip runs `All pages, URLs,
   Page titles, Meta descriptions, Headings, Duplicates, Canonicals, Broken,
   Redirects, Not indexable, Sitemap, Images, Response times` — `Duplicates`,
   `Broken` and `Not indexable` are findings wearing tab clothing.
4. **No crawl-mode selector.** SF has `Subdomain ▾` beside the URL field, which
   is the second most important crawl decision and takes one click.
5. **Progress eats table height.** SF spends 20px of status bar on
   `Spider Mode`, `URL/s` and `Completed n of m`; Pounce spends a whole strip.
6. **No `Selected Cells / Filter Total` readout**, and selection is row-based
   rather than cell-based, so copying a column of values is not a gesture.
7. **No per-pane filter row** in the detail pane.
8. **No chart under the panel.** SF has one on every single tab.

## 8. Where Pounce is better and must NOT copy

- **Findings as sentences.** SF says `H1: Multiple`. Pounce says what is wrong
  in words. Keep this; it is the agency-facing differentiator and it costs
  nothing in density.
- **"Not checked in this version."** SF has no equivalent, and a green zero
  there is genuinely ambiguous.
- **A real type scale and a warm palette.** SF has neither — one 11px size and
  one green.
- **Middle-truncated URLs keeping the tail.** SF clips from the right, which
  hides the part that distinguishes a row from its thousand siblings.
- **AA contrast in both themes, and a dark theme at all.** SF has no dark mode.

## 9. Recommendation

The brief as stated contains a tension worth naming: **Screaming Frog is not
warm and not inviting.** It is grey, cramped and plain. So "match Screaming Frog"
and "warm, inviting, good looking" cannot both mean the surface.

They are compatible if they are read as different layers:

> **Take Screaming Frog's skeleton and information architecture wholesale.
> Keep Pounce's voice, palette, type and finish.**

Warm does not mean sparse. The way to make a dense tool feel warm is not fewer
things on screen — it is better type, better colour and better spacing at the
*same* density. Screaming Frog proves the density works and is the thing every
SEO already knows how to drive; Pounce already has the type scale, the warm
neutrals and the contrast discipline that Screaming Frog lacks.

**The highest-value single change is #7.1** — rebuild the right panel as the
exhaustive filter index, because that panel *is* how Screaming Frog is navigated.
Doing that one thing delivers most of "it works like Screaming Frog", and it
does it without touching the palette, the type or the grid.

Ordered after that: the per-tab filter dropdown (#7.2), moving findings out of
the tab strip (#7.3), the crawl-mode selector (#7.4), and the status bar (#7.5).

---

# Part 2 — driving the live app (2026-09-05, with the owner's permission)

The screenshots cannot show menus, dropdowns, context menus or what a click
*does*. This section is from operating Screaming Frog 24.3 directly.

## 10. The loop — what one click on the Issues panel actually does

Clicking **`H1: Multiple`** in the Issues panel does **four things at once**:

1. The **left tab strip switches to `H1`**.
2. The **filter dropdown changes from `All` to `Multiple`**.
3. The **grid re-filters to the 10 offending URLs**, with H1-specific columns
   (`Occurrences | H1-1 | H1-1 Length | H1-2`).
4. The panel's lower half becomes an **`Issue Details`** pane — a `Copy` button,
   a `View: Details ▾` selector, and two headed sections:

   > **Description**
   > Pages which have multiple `<h1>`s. While this is not strictly an issue
   > because HTML5 standards allow multiple `<h1>`s on a page, there are some
   > problems with this modern approach in terms of usability. It's advised to
   > use heading rank (h1-h6) to convey document structure. […]
   >
   > **How To Fix**
   > Consider updating the HTML to include a single `<h1>` on each page, and
   > utilising the full heading rank (h2 - h6) for additional headings.

**This is the single most important interaction in the product.** One click
navigates, filters, re-columns and explains. Nothing is a dead end, and the
explanation arrives at the moment the offending rows do.

**Pounce already has this content and hides it.** `Overview.tsx` puts
`row.rule?.remediation` in a `title=` tooltip. The remediation text exists in
the rule registry and is surfaced as a hover hint instead of a pane.

## 11. The filter dropdown is the panel, scoped

Opening `All ▾` on the Page Titles tab gives exactly the panel's `Page Titles`
sub-list:

```
✓ All / Missing / Duplicate / Over 60 Characters / Below 30 Characters /
  Over 561 Pixels / Below 200 Pixels / Same as H1 / Multiple / Outside <head>
```

Same model, two surfaces — the panel for scanning across the whole crawl, the
dropdown for switching within the tab you are already on. They stay in sync.

## 12. The menu bar — 13 menus the screenshots never showed

`File · View · Mode · Configuration · Bulk Export · Reports · Sitemaps ·
Visualisations · Crawl Analysis · MCP · Licence · Window · Help`

- **View** is tiny and telling: `Reset Columns for All Tables`, `Reset Tabs`,
  `Focus Mode`. Two of the three exist to undo customisation — the app assumes
  you will rearrange it and will want out.
- **Configuration:** `Crawl Config ⌘,` · `Spider ▸` · `Content ▸` · `robots.txt` ·
  `URL Rewriting` · `CDNs` · `Include` · `Exclude` · `Speed` · `User-Agent` ·
  `HTTP Header` · `Custom ▸` · `API Access ▸` · `Authentication ▸` · `Segments` ·
  `Crawl Analysis` · `Profiles ▸`. Note **Speed** is top-level, and **Profiles**
  are saved configurations — Pounce has no equivalent of either.
- **Reports** is 17 cross-cutting exports that belong to no tab: `Crawl Overview`,
  `Issues Overview`, `Segments Overview`, `Redirects ▸`, `Canonicals ▸`,
  `Pagination ▸`, `Hreflang ▸`, `Insecure Content`, `SERP Summary`,
  `Orphan Pages`, `Structured Data ▸`, `Javascript ▸`, `PageSpeed ▸`, `Mobile ▸`,
  `Accessibility ▸`, `HTTP Headers ▸`, `Cookies ▸`. This is where Pounce's
  PDF/DOCX/XLSX reports belong conceptually.

## 13. The row context menu

Right-clicking a URL: `Copy` · `Open in Browser` · `Re-Spider` · `Remove` ·
`Export ▸` · `Visualisations ▸` · `Check Index ▸` · `Backlinks ▸` ·
`Validation ▸` · `History ▸` · `Speed ▸` · `Show Other Domains on IP` ·
`Open robots.txt`.

**`Re-Spider` and `Remove` are per-row crawl actions** — re-fetch this one URL,
or drop it from the crawl. Pounce has no row actions at all, not even Copy.

## 14. Revised recommendation

Part 1's recommendation stands, with the target sharpened. The thing to build is
not "a panel that lists checks" — it is **the loop**:

> **panel row → switches tab → sets filter → re-columns grid → explains itself**

Build order, highest value first:

1. **The filter index panel**, replacing Overview/Issues as the primary right
   panel: every check, every state, count + `% of total`, grouped by aspect,
   zeros included, always visible.
2. **Click drives everything** — tab, filter, columns — in one gesture.
3. **An Issue Details pane** under it: Description + How To Fix, promoted out of
   the tooltip the remediation text already sits in.
4. **A per-tab filter dropdown** bound to that tab's own sub-list, kept in sync
   with the panel.
5. **Move findings out of the tab strip** (`Duplicates`, `Broken`,
   `Not indexable` become panel entries; the strip keeps aspects only).
6. **Row context menu** — Copy, Open in Browser, Re-crawl this URL.
7. **Status bar** for mode / URL-s / completed-remaining, freeing the strip's
   height back to the table.
