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
import { Overview, ruleLines, summaryLines } from "./Overview";
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
}: {
  handle: CrawlHandle;
  live: Live | null;
  rules: Map<string, RuleInfo>;
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
  const picker = useRef<HTMLDialogElement>(null);
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

  useEffect(refreshOverview, [handle.path]);

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

  const pages = live ? live.progress.written : handle.pages;

  // What the right panel enumerates for the view that is open. The whole-crawl
  // rows only appear on the views that are about the whole crawl; everywhere
  // else the panel is the rules for that aspect, zeroes included.
  const current = view ?? VIEWS[0]!;
  const panelGroups = [
    ...(current.panel === "summary" && contents ? summaryLines(contents) : []),
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
    <div className="flex min-h-0 flex-1">
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
              className="btn rounded-b-none border-transparent bg-transparent aria-pressed:border-border aria-pressed:border-b-transparent aria-pressed:bg-canvas"
            >
              {v.label}
            </button>
          ))}
          {!view && (
            <span className="btn rounded-b-none border-border border-b-transparent bg-canvas">
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
          <button
            onClick={() => picker.current?.showModal()}
            className="btn shrink-0"
          >
            Columns
          </button>
        </div>

        <dialog
          ref={picker}
          onClick={(e) => e.target === picker.current && picker.current?.close()}
          className="m-auto rounded-md border border-border bg-surface p-0 text-fg"
        >
          <div className="flex w-72 flex-col gap-3 p-4">
            <h2 className="text-lg font-semibold">Columns</h2>
            <ul className="flex flex-col gap-1">
              {COLUMNS.map((column) => (
                <li key={column.key}>
                  <label className="flex items-center gap-2 text-md">
                    <input
                      type="checkbox"
                      checked={columns.includes(column.key)}
                      onChange={() =>
                        setColumns(saveColumns(toggleColumn(columns, column.key)))
                      }
                      className="focusable accent-accent"
                    />
                    {column.header}
                  </label>
                </li>
              ))}
            </ul>
            <div className="flex justify-between">
              <button
                onClick={() => setColumns(saveColumns(DEFAULT_COLUMNS))}
                className="btn"
              >
                Reset
              </button>
              <button onClick={() => picker.current?.close()} className="btn">
                Done
              </button>
            </div>
          </div>
        </dialog>

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
          refreshKey={refreshKey}
          selectedId={opened}
          onOpen={(row) => setOpened(row.id)}
          registerFocus={(focus) => {
            gridFocus.current = focus;
          }}
          emptyMessage={
            live
              ? "No rows yet — pages reach the file 500 at a time, and the first batch has not landed."
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
          <span className="nums ml-auto text-sm text-fg-faint">
            {view ? view.label : "Custom view"}
          </span>
        </footer>

        {opened !== null && (
          <Detail
            id={opened}
            rules={rules}
            onClose={() => {
              setOpened(null);
              // Back where the keyboard was. Closing a pane that took focus
              // and leaving focus on nothing is how a keyboard user loses
              // their place in a list of half a million rows.
              gridFocus.current?.();
            }}
          />
        )}
      </main>

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
    </div>
  );
}
