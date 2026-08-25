# Architecture

Pounce is a crawler that has to stay responsive while holding a million rows.
Nearly every decision below follows from one idea, and the rest of this document
is that idea and its consequences.

## The load-bearing decision: query, don't dump

**The UI never receives the crawl dataset.** It sends queries; SQLite returns
the visible window — about 200 rows. Sorting and filtering are SQL against
indexed columns. Scroll position maps to `OFFSET`.

The intuitive alternative is to crawl into memory, serialise the result over the
IPC bridge, and render it in React. That design works beautifully on the 500-page
test crawl a developer runs while building it, and collapses somewhere around
100,000 rows — where the JSON payload alone is hundreds of megabytes, the
renderer holds a second copy, and sorting means sorting an array of a million
objects in JavaScript on the main thread.

The whole product is a speed claim. A Rust crawler that feels slower than the
Electron competitor because of how its results are delivered would be a failure
that looks like an implementation detail. It is documented in the design spec as
**the single most likely way this project fails**, and every part of the system
below is arranged so it cannot happen by accident.

What follows from it:

- **Storage is disk-backed from the first row.** There is no in-memory mode to
  fall back to. This is also what makes crawls resumable and `.pounce` files
  portable.
- **The grid holds at most twelve windows** (`MAX_WINDOWS`, ~2,400 rows) and
  evicts by distance from the viewport. That constant *is* the invariant,
  written in the one place it could quietly stop being true.
- **Every sortable column carries an index**, and the `(filter, sort)` pairs the
  engine will run are enumerated rather than assumed — see *The query layer*.
- **Exports stream.** One statement, one `Write`, one row alive between them:
  500,000 rows to a 149 MB JSON file peaks at 10.2 MB of resident memory.
- **The detail pane caps its link lists** at 100 each with the true counts
  beside them. The same invariant, applied to one page rather than to a crawl.

## The shape of the system

A Cargo workspace of small crates. The GUI is one consumer of the engine, never
its owner: the CLI and the desktop app are peers over the same core.

```
pounce-core      orchestrator, frontier, scheduler, crawl lifecycle
pounce-http      fetch pool, retries, redirect chains, robots.txt, rate limits
pounce-parse     streaming extraction → PageRecord
pounce-store     SQLite schema, batched writer, query layer, resume state
pounce-audit     rule registry; each check is one testable unit
pounce-export    CSV / JSON, streamed
pounce-run       the crawl runner: frontier loop, image pass, lifecycle wiring
pounce-bench     fixture site + benchmark runner
pounce-cli       headless binary, CI exit codes
pounce-app       Tauri commands and events (thin)
ui/              React + TanStack Virtual, virtualised grid
```

## The pipeline

```
frontier → fetch pool → parse → batched writer (~500/txn) → audit rules
```

Bounded channels sit between every stage, so a slow disk throttles the fetchers
instead of ballooning memory. That is the difference between "we cap memory" and
"memory is capped": nothing anywhere accumulates in proportion to crawl size, and
the 500k measurement shows RSS flat at 144–174 MB while the database grew past
4 GB.

**A batch is not a crawl.** The runner feeds bounded slices of the frontier
through the pipeline; the pipeline deliberately does not end the crawl lifecycle,
because whoever owns the loop owns that call.

## The query layer

`FilterSpec` and `SortSpec` are the two halves of a query, and `SortSpec` can
only be built through a constructor that refuses pairs no index can serve. That
is not defensive coding — it is the difference between a 1.6 ms query and a
282 ms one, measured.

Three rules decide what is supported, each with a number behind it:

- a filter sorting by **its own column** needs nothing;
- an **equality** filter with a composite index is served by it (1.4–2 ms at 1M
  rows), and the composites are enumerated, not generated;
- a **range** filter walks the sort index and tests each row, so it is allowed
  only with the sort columns that were measured safe — which excludes the
  low-cardinality ones, where every skipped row is a table lookup landing
  somewhere else in the file.

The interface asks the engine which sorts are available for the filters
currently applied, rather than keeping a second list in TypeScript. When you
type into find-in-URL, every column header except URL greys out — because a
substring filter is only served with a URL sort, and no B-tree serves a
substring match.

## The row shape

`pages` is the narrow grid row; the six repeating JSON fields live in
`page_detail`. The split was measured before it was adopted, because it adds an
insert per page to the crawl's hot path: it made the write path **2.65% faster**,
not slower, because those bytes stop being dragged through a B-tree carrying
eight indices.

`pages.has_issue` is a denormalised cache of "this page has at least one
finding", because the grid's most-used filter was an `EXISTS` costing one
subquery per row the offset skipped — 220 ms at 1M, against 11–14 ms as an
equality the composites serve. `issues` remains the source of truth: the column
is rebuilt from it, and a test fails if one page disagrees.

**A new grid column belongs in `pages`; anything only the detail pane reads
belongs in `page_detail`.**

## Rules

Thirty of them, capped. A `PageRule` is handed a `&PageRecord` and nothing else
— it *cannot* issue a query, which is what keeps the audit's share of crawl wall
time enforceable by the compiler rather than by convention. Measured at 5.3%
against a 10% budget.

A `SiteRule` runs once at the end against indexed columns. That is not a second
pass over page bodies — the prohibition is on re-reading bodies — and the
distinction matters enough that the inlink index is built *before* the site
rules run: without it, one rule took 45 seconds on a 10,000-page store.

Every rule ships with a fixture that triggers it and one that does not. That is
what stops rule count becoming rule debt.

## Live results

The store is disk-backed from the first row and SQLite's WAL gives one writer
and many readers, so the app opens the file **while the crawl is still writing
it** — read-only, on a second connection, re-queried once a second. Read-only is
the guarantee rather than good manners: `Store::open` runs migrations, and two
connections racing `PRAGMA user_version` is a two-writers hazard reached from a
different direction.

Rows land in batches of 500, so a polite crawl really does spend its first
half-minute with pages fetched and no rows visible. Both empty states say so.

## Things that look like details and are not

- **Auto-redirect is disabled in reqwest.** The chain is data the crawler
  records, not plumbing to follow transparently.
- **`CrawlUrl`'s serde is hand-written.** Deserialisation re-parses, so a
  hand-edited `.pounce` file cannot produce one that skipped validation. The
  type's whole value is that holding one proves the checks ran.
- **Absent is not empty, anywhere.** A missing `<title>` and `<title></title>`
  are different findings, and `Option<String>` carries that distinction into SQL
  as NULL vs `''`, out through the detail pane, and into the JSON export. CSV
  cannot express it, which is documented rather than papered over.
- **Content is classified, never sniffed into.** Magic bytes may *contradict* a
  declared `Content-Type` and set a flag; they never override it. Silently
  trusting the bytes would hide the server misconfiguration that is the finding.
- **Politeness defaults are correctness.** robots.txt honoured with no way to
  turn it off, per-host concurrency caps, `Retry-After` respected, an honest
  user agent carrying a project URL. A fast crawler that gets its users
  IP-banned is a liability.
- **Progress events are throttled to ~10 Hz** in the engine, not in the shell —
  so the CLI and any future consumer inherit the throttle rather than being free
  to ignore it.
- **Fixture determinism does not depend on third-party crates.** `pounce-bench`
  uses a hand-rolled SplitMix64: a fixture site that reshaped itself on a
  dependency bump would invalidate every historical benchmark.

## Where the numbers live

`docs/benchmarks/`, newest last, each with the command that produced it. A
performance claim without one of these files behind it is not a claim this
project makes.
