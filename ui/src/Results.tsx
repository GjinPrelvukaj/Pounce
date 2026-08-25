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
import { useDelayed } from "./useDelayed";
import { IssueList, selectionFilters, type IssueSelection } from "./Issues";
import {
  issueOverview,
  supportedSorts,
  type CrawlHandle,
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
/// This is Screaming Frog's arrangement rather than its components: tabs that
/// are *saved questions*, not screens. Each one is a filter the engine already
/// serves, so switching tabs is a query, not a mode.
const VIEWS: { id: string; label: string; filters: FilterState }[] = [
  { id: "all", label: "All pages", filters: NO_FILTERS },
  {
    id: "broken",
    label: "Broken",
    filters: { ...NO_FILTERS, statusClass: "bad" },
  },
  {
    id: "redirects",
    label: "Redirects",
    filters: { ...NO_FILTERS, statusClass: "3" },
  },
  {
    id: "noindex",
    label: "Not indexable",
    filters: { ...NO_FILTERS, indexable: "no" },
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
  const [columns, setColumns] = useState<string[]>(storedColumns);
  const picker = useRef<HTMLDialogElement>(null);

  const filters = useMemo(
    () => [...selectionFilters(selection), ...toFilters(bar)],
    [selection, bar],
  );
  const filterKey = JSON.stringify(filters);
  const barKey = JSON.stringify(bar);
  const view = VIEWS.find((v) => JSON.stringify(v.filters) === barKey);

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
    issueOverview()
      .then(setOverview)
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
  // The overview is a `GROUP BY` over every issue in the file, so on a large
  // crawl it is the one query worth a placeholder — and on a small one it
  // answers before the placeholder is allowed to appear.
  const counting = useDelayed(overview === null);

  return (
    <div className="flex min-h-0 flex-1">
      {/* The rail is the summary and the navigation at once: it answers "what
          is wrong with this site" without reading a table, and every line in it
          opens the pages it counts. */}
      <aside className="flex w-80 min-w-0 shrink-0 flex-col gap-2 overflow-auto border-r border-border bg-surface p-3">
        <h2 className="text-sm font-medium text-fg-muted">What to fix</h2>
        {overview ? (
          <IssueList
            overview={overview}
            rules={rules}
            selection={selection}
            live={live !== null}
            onSelect={setSelection}
          />
        ) : counting ? (
          <div className="flex flex-col gap-2" aria-hidden>
            {Array.from({ length: 6 }, (_, i) => (
              <div
                key={i}
                className="h-3 rounded-sm bg-raised-2"
                style={{ width: `${85 - i * 8}%` }}
              />
            ))}
          </div>
        ) : null}
      </aside>

      <main className="flex min-w-0 min-h-0 flex-1 flex-col">
        <div className="flex flex-wrap items-center gap-1 border-b border-border px-3 pt-2">
          {VIEWS.map((v) => (
            <button
              key={v.id}
              onClick={() => setBar(v.filters)}
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

        <div className="flex flex-wrap items-center gap-2 px-3 py-2">
          <FilterBar value={bar} onChange={setBar} />
          <button
            onClick={() => picker.current?.showModal()}
            className="btn ml-auto"
          >
            Columns
          </button>
        </div>

        <dialog
          ref={picker}
          onClick={(e) => e.target === picker.current && picker.current?.close()}
          className="m-auto rounded-md border border-border bg-surface p-0 text-fg backdrop:bg-black/40"
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

        <p className="tabular px-3 pb-2 text-sm text-fg-muted">
          {filters.length === 0
            ? `${pages.toLocaleString()} pages${live ? " so far" : ""}`
            : `Showing ${total.toLocaleString()} of ${pages.toLocaleString()} pages`}
          {selection !== null && (
            <>
              {" — "}
              {selection === "*"
                ? "every page with something to fix"
                : (rules.get(selection)?.description ?? selection)}{" "}
              <button
                onClick={() => setSelection(null)}
                className="btn ml-1 px-1.5 py-0.5"
              >
                Clear finding
              </button>
            </>
          )}
        </p>

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

        {opened !== null && (
          <Detail id={opened} rules={rules} onClose={() => setOpened(null)} />
        )}
      </main>
    </div>
  );
}
