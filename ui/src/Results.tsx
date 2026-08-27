import { useEffect, useMemo, useRef, useState } from "react";
import { Detail } from "./Detail";
import {
  FilterBar,
  NO_FILTERS,
  toFilters,
  type FilterState,
} from "./Filters";
import { COLUMNS, Grid } from "./Grid";
import { DEFAULT_COLUMNS, saveColumns, storedColumns, toggleColumn } from "./columns";
import { selectionFilters, type IssueSelection } from "./Issues";
import type { Command } from "./CommandPalette";
import { ago, basename, type Recent } from "./recents";
import { Overview, ruleLines, summaryLines } from "./Overview";
import * as Menu from "@radix-ui/react-dropdown-menu";
import { Group, Panel, Separator, useDefaultLayout } from "react-resizable-panels";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import {
  crawlOverview,
  exportRows,
  issueOverview,
  supportedSorts,
  type CrawlHandle,
  type CrawlOverview,
  type IssueOverview,
  type ProgressEvent,
  type RuleInfo,
  type SortColumn,
} from "./engine";

/// A crawl in flight: the file being written and the last tick that described
/// it.
export type Live = { path: string; progress: ProgressEvent };

/// Views over one crawl.
///
/// This is the shape worth taking from Screaming Frog: a tab is not a screen,
/// it is **a saved question with its own columns**. "Page titles" is the same
/// rows as "All pages" with the title, its length and the word count brought
/// forward and the bytes dropped, because when you are auditing titles those
/// are the four things you look at.
///
/// Each one is a `FilterState` the engine already serves plus a column list the
/// grid already knows, so switching tabs is a query and a re-render — no mode,
/// no second code path.
const VIEWS: {
  id: string;
  label: string;
  filters: FilterState;
  columns: string[];
  /// The rule batches this view's panel enumerates. `title` gives the Page
  /// Titles panel its Missing / Duplicate / Too long rows — the rules *are*
  /// the filter list, which is the thing Screaming Frog's right panel is and
  /// the first version of this panel was not.
  batches: string[];
  /// `summary` adds the crawl-composition rows above the rules.
  panel?: "summary";
}[] = [
  {
    id: "all",
    label: "All pages",
    filters: NO_FILTERS,
    columns: ["row", "status", "url", "title", "wordCount", "depth", "size"],
    batches: [],
    panel: "summary",
  },
  {
    id: "titles",
    label: "Page titles",
    filters: { ...NO_FILTERS, kind: "html" },
    columns: ["row", "status", "url", "title", "titleLength", "wordCount"],
    batches: ["title"],
  },
  {
    id: "descriptions",
    label: "Meta descriptions",
    filters: { ...NO_FILTERS, kind: "html" },
    columns: ["row", "status", "url", "metaDescription", "descriptionLength"],
    batches: ["description"],
  },
  {
    id: "canonicals",
    label: "Canonicals",
    filters: { ...NO_FILTERS, kind: "html" },
    columns: ["row", "status", "url", "canonical", "noindex"],
    batches: ["indexability"],
  },
  {
    id: "broken",
    label: "Broken",
    filters: { ...NO_FILTERS, statusClass: "bad" },
    columns: ["row", "status", "url", "title", "depth"],
    batches: ["response"],
  },
  {
    id: "redirects",
    label: "Redirects",
    filters: { ...NO_FILTERS, statusClass: "3" },
    columns: ["row", "status", "url", "title", "depth"],
    batches: ["response"],
  },
  {
    id: "noindex",
    label: "Not indexable",
    filters: { ...NO_FILTERS, indexable: "no" },
    columns: ["row", "status", "url", "title", "canonical"],
    batches: ["indexability"],
  },
  {
    id: "images",
    label: "Images",
    filters: { ...NO_FILTERS, kind: "image" },
    columns: ["row", "status", "url", "size", "elapsedMs"],
    batches: ["media"],
  },
  {
    id: "slowest",
    label: "Response times",
    filters: NO_FILTERS,
    columns: ["row", "status", "url", "elapsedMs", "size"],
    batches: ["response"],
    panel: "summary",
  },
];

/// The results screen: findings on the left as navigation, one crawl's rows on
/// the right, the selected row underneath.
export function Results({
  handle,
  live,
  rules,
  recent,
  pace,
  onOpenRecent,
  onForgetRecent,
  onCommands,
}: {
  /// `null` before any crawl is open. The screen still renders in full: this
  /// is the difference between an application you can read at rest and one
  /// that hides behind a welcome page.
  handle: CrawlHandle | null;
  live: Live | null;
  rules: Map<string, RuleInfo>;
  recent: Recent[];
  pace: { text: string; heavy: boolean } | null;
  onOpenRecent: (path: string) => void;
  onForgetRecent: (path: string) => void;
  /// Publishes this screen's commands to the global palette.
  onCommands: (commands: Command[]) => void;
}) {
  const [overview, setOverview] = useState<IssueOverview | null>(null);
  const [contents, setContents] = useState<CrawlOverview | null>(null);
  const [total, setTotal] = useState(0);
  // Which finding the grid is filtered to. Held here rather than in the grid
  // because the rail and the grid are two views of one selection.
  const [selection, setSelection] = useState<IssueSelection>(null);
  // Bumped once a second while a crawl writes, which is what tells the grid
  // its cached windows describe an older state of the same file.
  const [refreshKey, setRefreshKey] = useState(0);
  const [bar, setBar] = useState<FilterState>(NO_FILTERS);
  const [sort, setSort] = useState<SortColumn>("url");
  const [direction, setDirection] = useState<"asc" | "desc">("asc");
  // Which sorts the engine will run against these filters. `undefined` while
  // the answer is in flight, which is not the same as "none" — greying every
  // header for a moment on each keystroke would be worse than a brief guess.
  const [sorts, setSorts] = useState<SortColumn[] | undefined>(undefined);
  // The row the detail pane is showing. An id, not a row: the pane fetches
  // everything itself, and a row object here would go stale the moment the
  // crawl rewrote that page.
  const [opened, setOpened] = useState<number | null>(null);
  // Seeded from what this person last chose, then owned by whichever view is
  // selected. The picker still writes the preference; a view does not, because
  // "I looked at titles once" is not a preference.
  const [columns, setColumns] = useState<string[]>(storedColumns);
  const gridFocus = useRef<(() => void) | null>(null);
  // What the last export did, shown beside the button. A file written silently
  // is a file the user goes looking for.
  const [exported, setExported] = useState<string | null>(null);

  async function exportView() {
    const chosen = await saveDialog({
      defaultPath: "pounce-export.csv",
      filters: [
        { name: "CSV", extensions: ["csv"] },
        { name: "JSON", extensions: ["json"] },
      ],
    });
    if (typeof chosen !== "string") return;
    setExported("Writing…");
    try {
      const rows = await exportRows({ path: chosen, filters, sort, direction });
      setExported(`${rows.toLocaleString()} rows → ${chosen.split("/").pop()}`);
    } catch (e) {
      const api = e as { message?: string };
      setExported(api?.message ?? String(e));
    }
  }

  const filters = useMemo(
    () => [...selectionFilters(selection), ...toFilters(bar)],
    [selection, bar],
  );
  const filterKey = JSON.stringify(filters);
  const barKey = JSON.stringify(bar);
  // A view is its filters and its columns together: the same rows with a
  // different projection is a different question.
  const view = VIEWS.find(
    (v) =>
      JSON.stringify(v.filters) === barKey &&
      JSON.stringify(v.columns) === JSON.stringify(columns),
  );

  // `url` is the fallback because it is the one column supported against every
  // filter shape this build has — a substring filter is *only* offered with it.
  useEffect(() => {
    supportedSorts(filters)
      .then((allowed) => {
        setSorts(allowed);
        if (!allowed.includes(sort)) {
          setSort("url");
          setDirection("asc");
        }
      })
      .catch(() => setSorts(undefined));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [filterKey]);

  // One overview query at a time. The count is a `GROUP BY` over every issue in
  // the file, so on a large crawl it can take longer than the second between
  // ticks — and queuing them would put the reader in the writer's way rather
  // than out of it.
  const overviewBusy = useRef(false);
  function refreshOverview() {
    if (overviewBusy.current) return;
    overviewBusy.current = true;
    // One flight for both panels. They are read together and neither is worth
    // a second round trip on its own.
    Promise.all([issueOverview(), crawlOverview()])
      .then(([issues, contents]) => {
        setOverview(issues);
        setContents(contents);
      })
      .catch(() => {})
      .finally(() => {
        overviewBusy.current = false;
      });
  }

  useEffect(() => {
    if (handle) refreshOverview();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [handle?.path]);

  // Results while the crawl runs, ticked once a second rather than at the
  // progress rate: 10 Hz of `count(*)` over a growing table is the reader
  // competing with the writer.
  const liveSecond = live ? Math.floor(live.progress.elapsedMs / 1000) : 0;
  useEffect(() => {
    if (!live) return;
    setRefreshKey((k) => k + 1);
    refreshOverview();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [liveSecond]);

  const pages = live ? live.progress.written : (handle?.pages ?? 0);

  // Pane sizes are a preference, so they persist. `useDefaultLayout` reads and
  // writes localStorage and hands back the props the Group needs.
  const hLayout = useDefaultLayout({ id: "pounce.layout.h", storage: localStorage });
  const vLayout = useDefaultLayout({ id: "pounce.layout.v", storage: localStorage });

  // Publish this screen's commands to the palette. The views are static; the
  // findings depend on what the crawl actually contains, so a rule with no
  // occurrences is offered with a `0` beside it rather than hidden — the same
  // reasoning as the panel, where a check reporting a pass is information.
  useEffect(() => {
    const views: Command[] = VIEWS.map((v) => ({
      id: `view-${v.id}`,
      label: v.label,
      group: "Views",
      run: () => {
        setBar(v.filters);
        setColumns(v.columns);
        if (v.id === "slowest") {
          setSort("elapsedMs");
          setDirection("desc");
        }
      },
    }));
    const counts = new Map<string, number>();
    for (const row of overview?.byRule ?? []) counts.set(row.ruleId, row.urls);
    const findings: Command[] = [...rules.values()].map((r) => ({
      id: `rule-${r.id}`,
      label: r.description,
      group: "Findings",
      hint: (counts.get(r.id) ?? 0).toLocaleString(),
      run: () => setSelection(r.id),
    }));
    onCommands([
      ...views,
      ...findings,
      {
        id: "export",
        label: "Export this view…",
        group: "Actions",
        run: () => void exportView(),
      },
      {
        id: "clear",
        label: "Clear all filters",
        group: "Actions",
        run: () => {
          setBar(NO_FILTERS);
          setSelection(null);
        },
      },
    ]);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [overview, rules, onCommands]);

  // What the right panel enumerates for the view that is open. The whole-crawl
  // rows only appear on the views that are about the whole crawl; everywhere
  // else the panel is the rules for that aspect, zeroes included.
  const current = view ?? VIEWS[0]!;
  // Before a crawl the panel shows the same tree with zeros rather than
  // nothing. A zero is a fact about this crawl; an absent row teaches nobody
  // what the panel is for.
  const ZEROED: CrawlOverview = {
    crawled: 0,
    queued: 0,
    failed: 0,
    byKind: [["html", 0]],
    byClass: [0, 0, 0, 0, 0],
    indexable: 0,
    noindex: 0,
  };
  const panelGroups = [
    ...(current.panel === "summary" ? summaryLines(contents ?? ZEROED) : []),
    {
      title: "What to fix",
      rows: [
        {
          label: "Every page with something to fix",
          count: overview?.urlsWithIssues ?? 0,
          share:
            overview && pages > 0 ? overview.urlsWithIssues / pages : undefined,
          rule: "*",
        },
        ...ruleLines(current.batches, rules, overview, pages),
      ],
    },
  ];


  return (
    // Draggable splitters, which Screaming Frog has and we did not. Hand
    // rolling one is a pointer-capture, keyboard and persistence problem, so
    // this is `react-resizable-panels`: it ships arrow-key resizing on a
    // focused handle and `autoSaveId` remembers the layout per person.
    <Group orientation="horizontal" className="flex min-h-0 flex-1" {...hLayout}>
      <Panel id="main" defaultSize="78%" minSize="45%" className="flex min-w-0 flex-col">
        <main className="flex min-h-0 min-w-0 flex-1 flex-col">
        <div className="flex flex-wrap items-center gap-1 border-b border-border px-3 pt-2">
          {VIEWS.map((v) => (
            <button
              key={v.id}
              onClick={() => {
                setBar(v.filters);
                setColumns(v.columns);
                // Response times is the one view with an opinion about order:
                // it exists to answer "what is slow", and that is a sort.
                if (v.id === "slowest") {
                  setSort("elapsedMs");
                  setDirection("desc");
                }
              }}
              aria-pressed={view?.id === v.id}
              className="tab"
            >
              {v.label}
            </button>
          ))}
          {!view && (
            <span className="tab" aria-pressed="true">
              Custom
            </span>
          )}
        </div>

        {/* The filters wrap; the two buttons do not. Letting the whole row wrap
            put "Columns" on a line of its own on a 1,280px window, which reads
            as a control that belongs to nothing. */}
        <div className="flex items-start gap-2 px-3 py-2">
          <div className="min-w-0 flex-1">
            <FilterBar value={bar} onChange={setBar} />
          </div>
          {exported && (
            <span className="nums shrink-0 py-1.5 text-sm text-fg-muted">
              {exported}
            </span>
          )}
          <button onClick={() => void exportView()} className="btn shrink-0">
            Export…
          </button>
          {/* A menu, not a modal. Toggling a column is a two-second decision
              you make while looking at the table, and a modal that covers the
              table to change what the table shows is the wrong shape for it.
              Radix supplies the focus handling, the typeahead and Escape. */}
          <Menu.Root>
            <Menu.Trigger className="btn shrink-0">Columns</Menu.Trigger>
            <Menu.Portal>
              <Menu.Content className="menu" sideOffset={6} align="end">
                <Menu.Label className="menu-label">Show columns</Menu.Label>
                {COLUMNS.map((column) => (
                  <Menu.CheckboxItem
                    key={column.key}
                    className="menu-item"
                    checked={columns.includes(column.key as string)}
                    // Radix closes on select by default; a column picker is a
                    // list you tick several things in.
                    onSelect={(e) => e.preventDefault()}
                    onCheckedChange={() => {
                      const next = toggleColumn(columns, column.key as string);
                      setColumns(next);
                      saveColumns(next);
                    }}
                  >
                    <span className="w-3 shrink-0 text-accent-fg">
                      {columns.includes(column.key as string) ? "\u2713" : ""}
                    </span>
                    {column.header}
                  </Menu.CheckboxItem>
                ))}
                <Menu.Separator className="my-1 h-px bg-border" />
                <Menu.Item
                  className="menu-item"
                  onSelect={() => {
                    setColumns(DEFAULT_COLUMNS);
                    saveColumns(DEFAULT_COLUMNS);
                  }}
                >
                  <span className="w-3 shrink-0" />
                  Reset to defaults
                </Menu.Item>
              </Menu.Content>
            </Menu.Portal>
          </Menu.Root>
        </div>

        {/* The chrome above (tabs, filters) does not resize; only the grid and
            the pane below it trade height, which is what the splitter is for. */}
        <Group orientation="vertical" className="flex min-h-0 flex-1 flex-col" {...vLayout}>
        <Panel id="rows" defaultSize="68%" minSize="20%" className="flex min-h-0 flex-col">
        <Grid
          filters={filters}
          visible={columns}
          sort={sort}
          direction={direction}
          supportedSorts={sorts}
          onSort={(column) => {
            // Clicking the column already sorted reverses it; clicking another
            // starts that one ascending, which is what every table does and the
            // only behaviour nobody has to be told about.
            if (column === sort) {
              setDirection((d) => (d === "asc" ? "desc" : "asc"));
            } else {
              setSort(column);
              setDirection("asc");
            }
          }}
          enabled={handle !== null}
          emptyContent={
            recent.length > 0 ? (
              <div className="flex w-full max-w-lg flex-col gap-1">
                <h3 className="px-1 text-xs font-semibold tracking-[0.07em] text-fg-faint uppercase">
                  Recent crawls
                </h3>
                <ul className="flex flex-col">
                  {recent.map((r) => (
                    <li
                      key={r.path}
                      className="flex items-center gap-2 border-b border-border/60 py-1.5"
                    >
                      <button
                        onClick={() => onOpenRecent(r.path)}
                        title={r.path}
                        className="focusable nums min-w-0 flex-1 truncate rounded-sm text-left text-sm text-accent-fg hover:underline"
                      >
                        {basename(r.path)}
                      </button>
                      <span className="nums shrink-0 text-xs text-fg-faint">
                        {r.pages.toLocaleString()} pages · {ago(r.openedAt)}
                      </span>
                      <button
                        onClick={() => onForgetRecent(r.path)}
                        aria-label={`Remove ${basename(r.path)} from recent crawls`}
                        className="focusable shrink-0 rounded-sm px-1 text-xs text-fg-faint transition-colors duration-150 ease-state hover:text-critical"
                      >
                        ✕
                      </button>
                    </li>
                  ))}
                </ul>
              </div>
            ) : (
              <p className="max-w-md text-center text-sm text-fg-muted">
                Enter a website address above and press Start. Pounce checks
                thirty things about every page and keeps the whole crawl in a
                file you can reopen.
              </p>
            )
          }
          refreshKey={refreshKey}
          selectedId={opened}
          onOpen={(row) => setOpened(row.id)}
          registerFocus={(focus) => {
            gridFocus.current = focus;
          }}
          emptyMessage={
            live
              ? "No rows yet. Pages reach the file 500 at a time, and the first batch has not landed."
              : filters.length === 0
                ? "This crawl has no pages."
                : "No pages match these filters."
          }
          onClearFilters={
            filters.length > 0
              ? () => {
                  setBar(NO_FILTERS);
                  setSelection(null);
                }
              : undefined
          }
          onTotal={setTotal}
        />

        {/* The foot of the pane, where Screaming Frog keeps its counts: what
            the grid is showing, out of what, and why it is not showing the
            rest. */}
        <footer className="flex shrink-0 flex-wrap items-center gap-2 border-t border-border bg-surface px-3 py-1.5">
          <span className="nums text-sm text-fg-muted">
            {filters.length === 0
              ? `${pages.toLocaleString()} pages${live ? " so far" : ""}`
              : `Showing ${total.toLocaleString()} of ${pages.toLocaleString()} pages`}
          </span>
          {selection !== null && (
            <>
              <span className="text-sm text-fg-faint">·</span>
              <span className="text-sm text-fg">
                {selection === "*"
                  ? "every page with something to fix"
                  : (rules.get(selection)?.description ?? selection)}
              </span>
              <button
                onClick={() => setSelection(null)}
                className="btn px-1.5 py-0.5"
              >
                Clear finding
              </button>
            </>
          )}
          {handle === null && pace && (
            <span
              className={`text-sm ${pace.heavy ? "text-warning" : "text-fg-faint"}`}
            >
              {pace.text}
            </span>
          )}
          <span className="nums ml-auto text-sm text-fg-faint">
            {live
              ? `Crawling · ${live.progress.written.toLocaleString()} done · ${live.progress.queued.toLocaleString()} queued · ${live.progress.urlsPerSecond.toFixed(1)} URL/s`
              : handle
                ? basename(handle.path)
                : "Idle"}
          </span>
          <span className="nums shrink-0 text-sm text-fg-faint">
            {view ? view.label : "Custom view"}
          </span>
        </footer>
        </Panel>

        <Separator className="split split-h" />
        <Panel id="detail" defaultSize="32%" minSize="12%" className="flex min-h-0 flex-col">
        {/* Always present, even with nothing selected. The pane is part of
            the interface a new user reads at rest, not a thing that appears
            once they already know to click a row. */}
        <Detail
          id={opened}
          rules={rules}
          onClose={() => {
            setOpened(null);
            // Back where the keyboard was. Closing a pane that took focus and
            // leaving focus on nothing is how a keyboard user loses their
            // place in a list of half a million rows.
            gridFocus.current?.();
          }}
        />
        </Panel>
        </Group>
        </main>
      </Panel>

      <Separator className="split split-v" />

      <Panel id="panel" defaultSize="22%" minSize="14%" maxSize="45%" className="flex min-w-0 flex-col">
      {/* The overview on the right, which is where Screaming Frog puts it and
          where it belongs: the grid is the thing being read, and a panel that
          summarises it should not sit between the reader and the left edge. */}
      <Overview
        title={view ? view.label : "Custom view"}
        groups={panelGroups}
        selection={selection}
        activeFilters={barKey}
        onSelectRule={setSelection}
        onFilter={setBar}
      />
      </Panel>
    </Group>
  );
}
