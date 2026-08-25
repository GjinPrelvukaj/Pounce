import { useVirtualizer } from "@tanstack/react-virtual";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { queryRows, type Filter, type RowView, type SortColumn } from "./engine";

/// A column, as this grid needs one: a width, a heading, and how to draw a
/// cell.
///
/// PLAN.md said TanStack Table here, and it is not carrying its weight:
/// sorting, filtering and pagination are all server-side, and the row model
/// only ever holds the visible window, so the library would contribute column
/// descriptors and a render helper — the twelve lines below — in exchange for
/// an API that changed shape between its last two majors. `react-virtual`
/// stays: measurement, overscan and scroll maths are real work.
export type Column = {
  key: keyof RowView;
  header: string;
  width: number;
  /// The engine's name for this column, when it is one you can sort by. A
  /// column with no `sort` is not sortable — there is no index behind it, and
  /// offering the click would be offering a full scan.
  sort?: SortColumn;
  render: (row: RowView) => React.ReactNode;
};

/// Rows per request. The store clamps a window at 1,000; 200 is what fits a
/// tall window with overscan, and asking for less more often beats asking for
/// more and throwing most of it away.
const WINDOW = 200;

/// How many windows may sit in memory at once — 2,400 rows, a few hundred
/// kilobytes.
///
/// This constant *is* the "UI never receives the dataset" invariant, expressed
/// in the one place it could quietly stop being true. Without eviction a long
/// scroll through 500,000 rows accumulates all of them, and the app would look
/// fine right up until someone opened a real crawl.
const MAX_WINDOWS = 12;

/// Fixed, and matching the design's row height. A virtualiser that has to
/// measure rows cannot know the scroll height before rendering them, which is
/// the thing that makes a million-row scrollbar honest.
const ROW_HEIGHT = 32;

export const COLUMNS: Column[] = [
  {
    key: "status",
    sort: "status",
    header: "Status",
    width: 70,
    render: (row) => {
      const tone =
        row.status >= 500
          ? "text-critical"
          : row.status >= 400
            ? "text-warning"
            : row.status >= 300
              ? "text-notice"
              : "text-pass";
      return <span className={`tabular ${tone}`}>{row.status}</span>;
    },
  },
  {
    key: "url",
    sort: "url",
    header: "URL",
    width: 620,
    render: (row) => (
      <span className="tabular block truncate text-accent-fg">{row.url}</span>
    ),
  },
  {
    key: "title",
    sort: "title",
    header: "Title",
    width: 300,
    // Absent is not empty — the store keeps that distinction all the way from
    // the parser, and the grid is where a user finally sees it.
    render: (row) =>
      row.title === null ? (
        <span className="text-fg-faint">— none</span>
      ) : row.title === "" ? (
        <span className="text-warning">— empty</span>
      ) : (
        <span className="block truncate">{row.title}</span>
      ),
  },
  {
    key: "wordCount",
    sort: "wordCount",
    header: "Words",
    width: 90,
    render: (row) => (
      <span className="tabular text-fg-muted">
        {row.wordCount.toLocaleString()}
      </span>
    ),
  },
  {
    key: "depth",
    sort: "depth",
    header: "Depth",
    width: 70,
    render: (row) => <span className="tabular text-fg-muted">{row.depth}</span>,
  },
  {
    key: "size",
    sort: "size",
    header: "Bytes",
    width: 90,
    render: (row) => (
      <span className="tabular text-fg-muted">{row.size.toLocaleString()}</span>
    ),
  },
];

export function Grid({
  filters,
  sort,
  direction,
  supportedSorts,
  onSort,
  refreshKey = 0,
  emptyMessage = "No pages match this filter.",
  onTotal,
}: {
  filters: Filter[];
  sort: SortColumn;
  direction: "asc" | "desc";
  /// Which sorts the engine will run against the filters currently applied.
  /// Asked of the engine rather than kept as a second list here: the pairs are
  /// decided by which composite indices exist, and a copy in TypeScript would
  /// drift the first time one is added.
  supportedSorts?: SortColumn[];
  onSort?: (column: SortColumn) => void;
  /// Bumped when the file underneath has changed — a crawl is writing into it.
  /// Everything cached describes an older state of the same query.
  refreshKey?: number;
  /// What to say when the query matched nothing. The grid cannot know whether
  /// that means "this filter is empty" or "the crawl has not saved a batch
  /// yet", and a blank rectangle says neither.
  emptyMessage?: string;
  onTotal?: (total: number) => void;
}) {
  const [total, setTotal] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [version, setVersion] = useState(0);

  // Windows are held in a ref, not in state: a fetch that lands should repaint
  // the rows, not rebuild the virtualiser's measurements.
  const windows = useRef(new Map<number, RowView[]>());
  const inflight = useRef(new Set<number>());
  const scroller = useRef<HTMLDivElement>(null);

  const key = useMemo(
    () => JSON.stringify({ filters, sort, direction }),
    [filters, sort, direction],
  );

  // The query this cache describes. A refresh keeps the scroll position; a
  // *changed* query does not, because row 40,000 of one filter has nothing to
  // do with row 40,000 of another.
  const lastKey = useRef(key);

  // A new query is a new dataset: everything cached describes the old one. A
  // live crawl reaches here too — same query, older answer.
  useEffect(() => {
    const changed = lastKey.current !== key;
    lastKey.current = key;
    windows.current.clear();
    inflight.current.clear();
    setError(null);
    if (changed) scroller.current?.scrollTo({ top: 0 });
    // Window 0 whether or not it is on screen: this is the call that carries
    // `total`, and during a crawl the total is the number that is moving.
    queryRows({ filters, sort, direction, offset: 0, limit: WINDOW })
      .then((page) => {
        windows.current.set(0, page.rows);
        setTotal(page.total);
        onTotal?.(page.total);
        setVersion((v) => v + 1);
      })
      .catch((e) => {
        const api = e as { message?: string; sort?: string; filter?: string };
        setError(
          api?.sort
            ? `sorting by ${api.sort} is not offered with a ${api.filter} filter`
            : (api?.message ?? String(e)),
        );
        setTotal(0);
      });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, refreshKey]);

  const virtualizer = useVirtualizer({
    count: total,
    getScrollElement: () => scroller.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 16,
  });

  const items = virtualizer.getVirtualItems();

  /// Fetches the windows the viewport needs and drops the ones it does not.
  const reconcile = useCallback(() => {
    if (items.length === 0) return;
    const first = Math.floor(items[0]!.index / WINDOW);
    const last = Math.floor(items[items.length - 1]!.index / WINDOW);

    for (let w = first; w <= last; w++) {
      if (windows.current.has(w) || inflight.current.has(w)) continue;
      inflight.current.add(w);
      queryRows({
        filters,
        sort,
        direction,
        offset: w * WINDOW,
        limit: WINDOW,
      })
        .then((page) => {
          windows.current.set(w, page.rows);
          setVersion((v) => v + 1);
        })
        .catch(() => {})
        .finally(() => inflight.current.delete(w));
    }

    // Evict by distance from the viewport rather than by age: scrolling back up
    // should not have to re-fetch what is directly above.
    if (windows.current.size > MAX_WINDOWS) {
      const middle = (first + last) / 2;
      [...windows.current.keys()]
        .sort((a, b) => Math.abs(b - middle) - Math.abs(a - middle))
        .slice(0, windows.current.size - MAX_WINDOWS)
        .forEach((w) => windows.current.delete(w));
    }
  }, [items, filters, sort, direction]);

  useEffect(reconcile, [reconcile]);

  // Read so the compiler knows what the repaint depends on: a landed window
  // changes `windows.current`, which React cannot see, so `version` is the
  // signal that the rows below are worth drawing again.
  void version;

  const rowAt = (index: number): RowView | undefined =>
    windows.current.get(Math.floor(index / WINDOW))?.[index % WINDOW];

  const templateColumns = COLUMNS.map((c) => `${c.width}px`).join(" ");

  if (error) {
    return <p className="tabular px-4 py-3 text-xs text-critical">{error}</p>;
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div
        className="grid border-b border-border bg-surface px-4"
        style={{ gridTemplateColumns: templateColumns }}
      >
        {COLUMNS.map((column) => {
          const sortable =
            column.sort !== undefined &&
            onSort !== undefined &&
            (supportedSorts === undefined || supportedSorts.includes(column.sort));
          const active = column.sort === sort;
          if (!sortable) {
            return (
              <div
                key={column.key}
                title={
                  column.sort && supportedSorts
                    ? `Sorting by ${column.header.toLowerCase()} is not offered with the filters applied — no index serves that pair, and the query would scan the whole crawl.`
                    : undefined
                }
                className="py-1.5 text-left text-xs font-medium text-fg-faint/60"
              >
                {column.header}
              </div>
            );
          }
          return (
            <button
              key={column.key}
              onClick={() => onSort(column.sort!)}
              aria-sort={
                active
                  ? direction === "asc"
                    ? "ascending"
                    : "descending"
                  : "none"
              }
              className={`flex items-center gap-1 py-1.5 text-left text-xs font-medium transition-colors duration-150 ease-state focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent ${
                active ? "text-fg" : "text-fg-faint hover:text-fg"
              }`}
            >
              {column.header}
              <span aria-hidden className={active ? "" : "opacity-0"}>
                {direction === "asc" ? "\u2191" : "\u2193"}
              </span>
            </button>
          );
        })}
      </div>

      {total === 0 && (
        <p className="px-4 py-3 text-xs text-fg-muted">{emptyMessage}</p>
      )}

      <div ref={scroller} className="min-h-0 flex-1 overflow-auto px-4">
        <div
          style={{ height: virtualizer.getTotalSize(), position: "relative" }}
        >
          {items.map((item) => {
            const row = rowAt(item.index);
            return (
              <div
                key={item.key}
                className="grid items-center border-b border-border/50 text-xs"
                style={{
                  position: "absolute",
                  top: 0,
                  left: 0,
                  width: "100%",
                  height: ROW_HEIGHT,
                  transform: `translateY(${item.start}px)`,
                  gridTemplateColumns: templateColumns,
                }}
              >
                {row ? (
                  COLUMNS.map((column) => (
                    <div key={column.key} className="min-w-0 pr-3">
                      {column.render(row)}
                    </div>
                  ))
                ) : (
                  // A row whose window has not landed yet. Deliberately quiet:
                  // a spinner per row would make fast scrolling look broken.
                  <div className="col-span-full h-3 w-40 rounded-sm bg-raised-2" />
                )}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
