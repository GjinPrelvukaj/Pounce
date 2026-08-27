---
name: Pounce
description: A natively compiled technical SEO crawler whose interface is a surveyor's desk, not a terminal.
colors:
  signal-violet: "oklch(55.4% 0.2358 283.5)"
  signal-violet-text-light: "oklch(49.5% 0.2176 283.2)"
  signal-violet-text-dark: "oklch(74.4% 0.1421 290.3)"
  warm-paper: "oklch(96.8% 0.005 283.5)"
  paper-raised: "oklch(99.2% 0.003 283.5)"
  paper-sunken: "oklch(94.6% 0.006 283.5)"
  graphite: "oklch(23.5% 0.010 283.5)"
  graphite-muted: "oklch(45.0% 0.014 283.5)"
  ash: "oklch(53.5% 0.014 283.5)"
  rule-line: "oklch(90.8% 0.008 283.5)"
  rule-line-strong: "oklch(84.0% 0.010 283.5)"
  night-desk: "oklch(20.5% 0.007 283.5)"
  night-raised: "oklch(23.5% 0.008 283.5)"
  night-ink: "oklch(95.0% 0.006 283.5)"
  severity-critical: "oklch(53.5% 0.1654 27.0)"
  severity-warning: "oklch(51.6% 0.1031 82.2)"
  severity-notice: "oklch(45.1% 0.0793 221.9)"
  severity-pass: "oklch(46.1% 0.1073 153.8)"
typography:
  display:
    fontFamily: "Inter Variable, -apple-system, BlinkMacSystemFont, Segoe UI, system-ui, sans-serif"
    fontSize: "1.25rem"
    fontWeight: 600
    lineHeight: 1.15
    letterSpacing: "-0.021em"
  title:
    fontFamily: "Inter Variable, -apple-system, system-ui, sans-serif"
    fontSize: "1rem"
    fontWeight: 500
    lineHeight: 1.3
    letterSpacing: "-0.011em"
  body:
    fontFamily: "Inter Variable, -apple-system, system-ui, sans-serif"
    fontSize: "0.8125rem"
    fontWeight: 400
    lineHeight: 1.45
    letterSpacing: "0em"
  label:
    fontFamily: "Inter Variable, -apple-system, system-ui, sans-serif"
    fontSize: "0.6875rem"
    fontWeight: 600
    lineHeight: 1.45
    letterSpacing: "0.07em"
  code:
    fontFamily: "JetBrains Mono Variable, ui-monospace, SFMono-Regular, Menlo, monospace"
    fontSize: "0.8125rem"
    fontWeight: 400
    lineHeight: 1.45
    letterSpacing: "0em"
rounded:
  sm: "8px"
  md: "12px"
  lg: "16px"
spacing:
  xs: "4px"
  sm: "6px"
  md: "8px"
  lg: "12px"
  xl: "16px"
components:
  button-default:
    backgroundColor: "{colors.paper-raised}"
    textColor: "{colors.graphite-muted}"
    typography: "{typography.body}"
    rounded: "{rounded.sm}"
    padding: "5.5px 11px"
  button-default-hover:
    textColor: "{colors.graphite}"
  button-primary:
    backgroundColor: "{colors.signal-violet}"
    textColor: "{colors.paper-raised}"
    typography: "{typography.body}"
    rounded: "{rounded.sm}"
    padding: "5.5px 11px"
  field:
    backgroundColor: "{colors.paper-raised}"
    textColor: "{colors.graphite}"
    typography: "{typography.body}"
    rounded: "{rounded.sm}"
    padding: "5.5px 9px"
  field-active:
    backgroundColor: "{colors.signal-violet}"
    textColor: "{colors.graphite}"
  tab:
    backgroundColor: "transparent"
    textColor: "{colors.graphite-muted}"
    typography: "{typography.body}"
    rounded: "{rounded.sm}"
    padding: "7px 11px"
  tab-selected:
    textColor: "{colors.graphite}"
  grid-row:
    backgroundColor: "transparent"
    textColor: "{colors.graphite}"
    typography: "{typography.body}"
    height: "38px"
  panel-row:
    backgroundColor: "transparent"
    textColor: "{colors.graphite-muted}"
    typography: "{typography.body}"
    rounded: "{rounded.sm}"
    padding: "4px 10px"
---

# Design System: Pounce

## 1. Overview

**Creative North Star: "The Surveyor's Desk"**

Instruments and field notes laid out on warm paper. A surveyor's desk is dense
because the work is dense, and calm because every object on it has a job and a
place. Nothing is decorative. Everything is legible at a glance, and the things
that need attention are the things you notice first.

Pounce shows a technical SEO crawl: hundreds of thousands of rows, thirty checks
per page, four levels of severity. The interface exists to make that legible
without making it loud. It is warm rather than cool, because the previous cool
blue-black build read as "a TUI tool made for hackers" to the person who
specified it. Warmth here is not softness: the density is unchanged, the grid
still shows everything at once, and the numbers still dominate.

This system explicitly rejects the terminal. It rejects monospace as a default
voice, near-black grounds, hairline radii, and the flat single-plane layout of a
text-mode application. It equally rejects the SaaS marketing register: no hero
metrics, no gradient text, no glass, no identical card grids. The audience is a
specialist at work and an agency colleague reading over their shoulder, and both
are served by the same thing, which is clarity at density.

**Key Characteristics:**
- Warm neutral stack, every grey tinted toward the brand hue (283.5 degrees).
- A single accent, used only to mark the current thing.
- Severity is the loudest signal on screen, and never carried by colour alone.
- Four type steps, each with its own tracking and leading.
- Flat by region, lifted only where something genuinely floats.

## 2. Colors

A warm neutral field with one violet accent and a four-level severity vocabulary
that must never be confused with it.

### Primary
- **Signal Violet** (`oklch(55.4% 0.2358 283.5)`): Primary buttons, the selected
  grid row, the active tab underline, focus rings, and the fill behind a chosen
  filter. It marks *the current thing* and nothing else. As text it steps to
  `oklch(49.5% 0.2176 283.2)` on paper and `oklch(74.4% 0.1421 290.3)` at night,
  because the fill value is not legible as type against either ground.

### Secondary
Deliberately absent. Pounce has one accent. Severity supplies every other
meaningful colour, and adding a second brand hue would compete with it.

### Tertiary
- **Severity Critical** (`oklch(53.5% 0.1654 27.0)`): The page is broken. A 5xx,
  a redirect that never resolves, a canonical naming something impossible.
- **Severity Warning** (`oklch(51.6% 0.1031 82.2)`): A real defect on a page that
  otherwise works. Duplicated titles, thin content, a working noindex.
- **Severity Notice** (`oklch(45.1% 0.0793 221.9)`): Nothing is wrong; there is
  headroom. Cyan rather than blue so it cannot be read as the accent.
- **Severity Pass** (`oklch(46.1% 0.1073 153.8)`): A 2xx, a check that found
  nothing. Used for status codes in the grid, never for chrome.

### Neutral
- **Warm Paper** (`oklch(96.8% 0.005 283.5)`): The desk. The light canvas.
- **Paper Raised** (`oklch(99.2% 0.003 283.5)`): Panels, headers, controls. The
  sheet lying on the desk. There is no pure white anywhere in this system.
- **Paper Sunken** (`oklch(94.6% 0.006 283.5)`): Hover grounds and inset wells.
- **Graphite** (`oklch(23.5% 0.010 283.5)`): Primary text. Values, titles, data.
- **Graphite Muted** (`oklch(45.0% 0.014 283.5)`): Secondary text and control
  labels. The default resting colour of a button.
- **Ash** (`oklch(53.5% 0.014 283.5)`): Section labels, units, counts beside a
  label, and any figure that is context rather than content.
- **Rule Line** (`oklch(90.8% 0.008 283.5)`) and **Rule Line Strong**
  (`oklch(84.0% 0.010 283.5)`): Table rules, panel edges, control strokes. The
  strong step is also what `prefers-contrast: more` promotes every border to.
- **Night Desk** (`oklch(20.5% 0.007 283.5)`), **Night Raised**
  (`oklch(23.5% 0.008 283.5)`), **Night Ink** (`oklch(95.0% 0.006 283.5)`): The
  dark theme's equivalents. Warm charcoal, never blue-black.

### Named Rules

**The Signal Rule.** The accent marks the current thing: the selected row, the
active tab, the applied filter, the focused control. It is never decoration,
never a heading colour, never a border for emphasis. On a full results screen it
covers well under ten percent of the surface, and that rarity is what makes a
selection findable in a table of a million rows.

**The Severity Band Rule.** The brand colour is forbidden from the red, amber and
green band. This constraint has survived two complete redesigns and outranks any
aesthetic preference, because severity states dominate this interface and a brand
colour mistaken for a state is a misread finding.

**The Never Colour Alone Rule.** Every severity is drawn with an icon and a word
beside its hue. A reader who cannot separate red from amber still reads
"Critical". Remove the colour and the interface must still be correct.

**The Tinted Neutral Rule.** Every neutral carries chroma 0.004 to 0.014 at hue
283.5. Pure grey is forbidden, `#ffffff` and `#000000` are forbidden, and a
neutral tinted *away* from the accent is the specific failure this rule exists to
prevent: the first warm build put the greys at hue 84 against an accent at 283,
and the two read as different products.

## 3. Typography

**Display and Body Font:** Inter Variable (with `-apple-system`,
`BlinkMacSystemFont`, `Segoe UI`, `system-ui`, sans-serif)
**Code Font:** JetBrains Mono Variable (with `ui-monospace`, `SFMono-Regular`,
Menlo, monospace)

**Character:** One humanist sans carries the entire interface: headings, labels,
buttons, body and data. `font-optical-sizing: auto` lets Inter reshape itself
across the scale the way a system font does. Monospace appears only where a
person reads character by character, which in this product means a URL, a file
path, or a canonical.

### Hierarchy
- **Display** (600, 20px / 1.25rem, line-height 1.15, tracking -0.021em): The
  wordmark, and live crawl numbers meant to be read from further than a desk.
- **Title** (500, 16px / 1rem, line-height 1.3, tracking -0.011em): Panel titles
  and screen headings. The only step between body and display.
- **Body** (400, 13px / 0.8125rem, line-height 1.45, tracking 0): Grid rows,
  values, prose, controls, fields. The default; `body` sets it. Prose is capped
  at 65 to 75 characters; table rows run as wide as the data needs.
- **Label** (600, 11px / 0.6875rem, tracking 0.07em, uppercase): Panel section
  headings and grid column headings. Also the resting size for dense metadata
  (units, percentages, rule ids), in which case it is 400 weight, sentence case,
  and tracking 0.006em.
- **Code** (400, 13px, mono): URLs, paths, canonicals. Nothing else.

### Named Rules

**The Four Steps Rule.** Four sizes, never five. The previous scale ran
11/12/13/15/18 and put three steps within two pixels of each other, ratios of
1.09 and 1.08. Everything read at one volume and no repaint could fix it,
because the structure that carries hierarchy did not exist. The steps are now
11/13/16/20 at ratios 1.18, 1.23 and 1.25. Adding a fifth step requires deleting
one.

**The Size Owns Its Tracking Rule.** Tracking and leading are properties of the
size step, never global. Letters read too close at 11px (tracking goes positive,
0.006em) and too far apart at 20px (tracking goes negative, -0.021em); leading
tightens as size grows. A single global `letter-spacing` is wrong for at least
one size by definition, and this system had exactly that until it was measured.

**The Two Voices Rule.** Monospace is for text read character by character.
Counts, byte sizes, percentages and durations are *not* that; they need to line
up, which is a job for `font-variant-numeric: tabular-nums` on Inter. Forty-two
monospaced fragments in one window is what a terminal looks like, and that
number is not hypothetical: it is what this interface shipped with before the
rule existed.

## 4. Elevation

Depth is structural, not decorative, and it works two ways at once. Regions are
separated **tonally**: canvas, surface and raised are three steps of the same
warm neutral, so a panel is distinguished from the desk by lightness and a
hairline rule rather than by a shadow. Shadows are reserved for the two things
that genuinely sit above the page, plus a single hairline that makes controls
feel picked up rather than printed on.

### Shadow Vocabulary
- **Control** (`0 1px 2px oklch(23.5% 0.01 283.5 / 0.05), 0 1px 1px oklch(23.5%
  0.01 283.5 / 0.03)`): Buttons and fields at rest. Almost subliminal. Its job is
  to separate a control from the surface behind it, and it is removed on `:active`
  so a press reads as the control going down.
- **Panel** (`0 4px 12px -2px oklch(23.5% 0.01 283.5 / 0.08), 0 16px 40px -12px
  oklch(23.5% 0.01 283.5 / 0.14)`): The detail pane and modal dialogs. The dark
  theme uses the same geometry at higher opacity, because a shadow on a dark
  ground must work harder to be seen.

### Named Rules

**The Flat Until It Floats Rule.** A surface gets a shadow only if it genuinely
sits above the content, which in this product means exactly two things: the
detail pane and a modal dialog. Every other region is tonal. If a new panel wants
a shadow, the question to answer first is whether it is floating or merely
adjacent, and adjacent is the common answer.

**The Press Goes Down Rule.** `:active` removes the control shadow and darkens
the ground. A pressed control must read as pressed, not as hovered harder.

## 5. Components

Refined and restrained. Controls recede so the data is the loudest thing on
screen. Every interactive element carries the full state set: rest, hover, active,
focus-visible, disabled, and where applicable pressed.

### Buttons
- **Shape:** Gently curved (8px, `--radius-sm`).
- **Default:** Raised paper ground, muted graphite text, a one pixel rule line
  border, and the control shadow. Padding 5.5px by 11px, 13px text at weight 500.
- **Hover:** Border promotes to the strong rule line, text promotes to full
  graphite. Nothing moves.
- **Active:** Shadow removed, ground steps to sunken paper.
- **Focus:** A two pixel Signal Violet outline at one pixel offset. Never removed.
- **Disabled:** 50% opacity, `not-allowed` cursor, shadow removed.
- **Primary:** A vertical Signal Violet gradient (88% white-mixed at the top to
  the pure accent at the bottom) with an inset one pixel white highlight at 18%.
  This is the only gradient in the system and it exists to suggest a light source
  above the desk, not to decorate.
- **Pressed (`aria-pressed="true"`):** Accent-dim ground, accent-line border,
  full graphite text, shadow removed. This is how a toggle reads as on.

### Chips
Not used. Filter state lives in the fields themselves and in the right panel's
rows, so a separate chip vocabulary would be a third way to say the same thing.

### Cards / Containers
- **Corner Style:** 12px (`--radius-md`) for containers, 16px (`--radius-lg`) for
  floating panels and dialogs.
- **Background:** Paper Raised on Warm Paper.
- **Shadow Strategy:** None, unless floating. See Elevation.
- **Border:** One pixel Rule Line. This is what separates regions.
- **Internal Padding:** 16px (`spacing.xl`) for panels, 12px (`spacing.lg`) for
  toolbars and strips.

### Inputs / Fields
- **Style:** Raised paper ground, one pixel Rule Line stroke, 8px radius, control
  shadow, 13px text.
- **Hover:** Border promotes to strong rule line.
- **Focus:** Border becomes accent-line and a three pixel accent-dim ring appears
  outside it. The native outline is removed, but only because something replaces
  it.
- **Carrying a value (`.field-set`):** Accent-line border and accent-dim ground,
  shadow removed. An accidentally-set filter is an empty grid that looks like an
  answer, so a field holding a filter must be visible across the room.

### Navigation
- **Style:** Underline tabs. Transparent ground, muted graphite text, 13px at
  weight 500, 8px top corners.
- **Hover:** Sunken paper ground, full graphite text.
- **Selected:** Full graphite text plus a two pixel Signal Violet underline
  inset half a rem from each edge, sitting on the strip's own bottom rule.
- **Counts:** Tabs may carry a count in Ash at label size, separated by 6px.

### The Crawl Toolbar
Persistent, in the header, never dismissed. An address field, a pace selector,
Start, Clear and Options. Re-crawling is one field away and there is no setup
screen to navigate to and back from. The pace selector turns amber when the
current setting is heavy, so the consequence of the default is visible without
opening anything, and the full sentence lives in the status bar while idle.

**The Machine At Rest Rule.** Before any crawl exists, the whole interface is
on screen: every tab, the filter bar, the column headings, the panel tree with
zeros, "No data" in the grid, "No URL selected" in the pane, "Idle" in the
status bar. A person learns this tool by looking at it. A welcome screen,
however well written, teaches nothing about the application behind it, and the
first thing it teaches is that things are hidden.

### The Findings Panel Row
The signature component. A full-width button carrying a severity icon, a finding
written as a sentence, a count, and a percentage of the crawl, with a
**proportional bar behind it** in that finding's own severity colour at 9 to 12%
opacity. The bar folds the chart into the list: the panel reads at a glance the
way a bar chart does while remaining a list of buttons.

Rules that hold it together: a finding that occurred zero times is still listed,
greyed, because a check reporting a pass is information and silence is not the
same statement. The label is the rule's own `description` sentence, never its id.
The count is URLs, not occurrences, because the number on the row must be the
number of rows the click produces.

### The Grid Row
38px tall, 13px text, one pixel Rule Line beneath. URLs are monospace and
truncate from the **middle**, keeping the final path segment, because the tail is
what distinguishes a row from its thousand siblings. Numeric columns are right
aligned with right aligned headings; the row number column is the sole exception,
its heading left aligned so it does not collide with the next column. Hover
raises the ground to sunken paper; selection fills with accent-dim and an
accent-line border; the keyboard cursor is drawn as an inset accent outline,
separately from selection, because where the keyboard is and what the pane is
showing are two different facts.

## 6. Do's and Don'ts

### Do:
- **Do** author every colour in OKLCH, and tint every neutral toward hue 283.5 at
  chroma 0.004 to 0.014.
- **Do** pair every severity colour with an icon and a word.
- **Do** give each type step its own tracking and leading.
- **Do** use `.nums` (Inter tabular figures) for anything that must line up, and
  reserve `.tabular` (JetBrains Mono) for URLs, paths and canonicals.
- **Do** run `npm run check:contrast` after any palette change. It parses OKLCH,
  throws on an unparseable colour rather than scoring it, and fails the build.
- **Do** list a check that found nothing, at zero, greyed.
- **Do** write findings as sentences from the rule registry's own `description`.
- **Do** keep gutters uniform. Predictable grids are an affordance at this density.

### Don't:
- **Don't** make it look like "a TUI tool made for hackers". Named in PRODUCT.md,
  and the reason this system exists. No near-black grounds, no monospace as a
  default voice, no hairline radii, no single-plane flatness.
- **Don't** move the accent into the red, amber or green band, ever.
- **Don't** use pure `#ffffff`, `#000000`, or any untinted grey.
- **Don't** use monospace for counts, sizes, durations or percentages.
- **Don't** add a fifth type step without deleting one.
- **Don't** use em dashes in interface copy. Parentheses for qualifiers
  ("Worked (2xx)"), colons or periods for clauses ("Stopped: URL limit reached").
- **Don't** reinvent a standard affordance. No custom scrollbars, no bespoke form
  controls, no hand-rolled modals. Use `<dialog>` and let the platform supply the
  backdrop, the focus trap and Escape. A restyled scrollbar shipped here once and
  was reverted.
- **Don't** remove a focus outline without putting something in its place.
- **Don't** give a region a shadow because it looks flat. Ask whether it floats.
- **Don't** use `border-left` above one pixel as a coloured accent stripe, gradient
  text, glassmorphism, hero-metric layouts, or identical card grids.
- **Don't** show a rule id where a finding belongs. If it looks like a database
  key, it is one, and it does not go in the interface.
