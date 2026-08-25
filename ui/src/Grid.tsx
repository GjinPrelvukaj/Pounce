import { useVirtualizer } from "@tanstack/react-virtual";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { MiddleTruncate } from "./MiddleTruncate";
import { useDelayed } from "./useDelayed";
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
  /// A CSS grid track. Fixed for the numeric columns — they are as wide as
  /// their widest value and no wider — and elastic for URL and title, which is
  /// what stops a 1,240px row from needing a horizontal scrollbar next to a
  /// 320px rail.
  track: string;
  /// Right-aligned, because a column of numbers you cannot compare by eye is a
  /// column you have to read one row at a time.
  numeric?: boolean;
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
const ROW_HEIGHT = 34;

export const COLUMNS: Column[] = [
  {
    key: "status",
    sort: "status",
    header: "Status",
    track: "4.5rem",
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
    // Elastic, and the widest column: a URL is the row's identity and every
    // other column is a fact about it.
    track: "minmax(18rem, 3fr)",
    render: (row) => (
      <span className="tabular block text-accent-fg">
        <MiddleTruncate text={row.url} />
      </span>
    ),
  },
  {
    key: "title",
    sort: "title",
    header: "Title",
    track: "minmax(12rem, 2fr)",
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
    key: "kind",
    header: "Type",
    track: "6rem",
    render: (row) => (
      <span className="text-fg-muted">
        {row.kind === "html"
          ? "Page"
          : row.kind === "pdf"
            ? "PDF"
            : row.kind === "image"
              ? "Image"
              : row.kind === "undeclared"
                ? "No type"
                : "Other"}
      </span>
    ),
  },
  {
    key: "noindex",
    header: "Indexable",
    track: "6.5rem",
    // Not sortable through this column: `noindex` is a filter with a composite
    // index, and the grid already offers it as one. A sort by a boolean is a
    // two-value ordering nobody scrolls through.
    render: (row) =>
      row.noindex ? (
        <span className="text-warning">No — noindex</span>
      ) : (
        <span className="text-fg-muted">Yes</span>
      ),
  },
  {
    key: "wordCount",
    sort: "wordCount",
    header: "Words",
    track: "5.5rem",
    numeric: true,
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
    track: "4.5rem",
    numeric: true,
    render: (row) => <span className="tabular text-fg-muted">{row.depth}</span>,
  },
  {
    key: "size",
    sort: "size",
    header: "Bytes",
    track: "6rem",
    numeric: true,
    render: (row) => (
      <span className="tabular text-fg-muted">{row.size.toLocaleString()}</span>
    ),
  },
];

/// Rows that are not there yet, at the shape they will be.
///
/// Deliberately not a spinner: the grid is about to be a list, and a list of
/// grey bars is the only placeholder that does not move the eye when the real
/// rows arrive.
function Skeleton() {
  return (
    <div className="flex flex-col gap-3 px-1 py-3" aria-hidden>
      {Array.from({ length: 8 }, (_, i) => (
        <div
          key={i}
          className="h-3 rounded-sm bg-raised-2"
          style={{ width: `${70 - (i % 4) * 12}%` }}
        />
      ))}
    </div>
  );
}

export function Grid({
  filters,
  visible,
  sort,
  direction,
  supportedSorts,
  onSort,
  refreshKey = 0,
  emptyMessage = "No pages match this filter.",
  onClearFilters,
  selectedId = null,
  onOpen,
  registerFocus,
  onTotal,
}: {
  filters: Filter[];
  /// Column keys to draw, in this order. Undefined means all of them.
  visible?: string[];
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
  /// Offered beside the empty message. An empty grid with no way out of it is
  /// the state people close the app in.
  onClearFilters?: () => void;
  /// The row the detail pane is showing, if any.
  selectedId?: number | null;
  onOpen?: (row: RowView) => void;
  /// Hands the caller a way to put the keyboard back here — used when the
  /// detail pane closes.
  registerFocus?: (focus: () => void) => void;
  onTotal?: (total: number) => void;
}) {
  const [total, setTotal] = useState(0);
  // Whether the count for *this* query has come back. `total === 0` means two
  // different things either side of it — "nothing matched" and "we have not
  // asked yet" — and showing the first while the second is true is how an
  // empty state flashes on every keystroke.
  const [counted, setCounted] = useState(false);
  // Where the keyboard is, which is not where the pane is. A specialist arrows
  // down a list reading it and opens one row in ten; conflating the two would
  // make every arrow key a query.
  const [cursor, setCursor] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [version, setVersion] = useState(0);

  // Windows are held in a ref, not in state: a fetch that lands should repaint
  // the rows, not rebuild the virtualiser's measurements.
  const windows = useRef(new Map<number, RowView[]>());
  const header = useRef<HTMLDivElement>(null);
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
    setCounted(false);
    if (changed) {
      scroller.current?.scrollTo({ top: 0 });
      setCursor(0);
    }
    // Window 0 whether or not it is on screen: this is the call that carries
    // `total`, and during a crawl the total is the number that is moving.
    queryRows({ filters, sort, direction, offset: 0, limit: WINDOW })
      .then((page) => {
        windows.current.set(0, page.rows);
        setTotal(page.total);
        setCounted(true);
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
        setCounted(true);
      });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, refreshKey]);

  const virtualizer = useVirtualizer({
    count: total,
    getScrollElement: () => scroller.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 16,
  });

  useEffect(() => {
    registerFocus?.(() => scroller.current?.focus());
  }, [registerFocus]);

  const items = virtualizer.getVirtualItems();

  // Only worth a placeholder if the answer is slow enough to notice.
  const waiting = useDelayed(!counted);


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

  // Ordered by the picker's list rather than by `COLUMNS`, so a future
  // reordering needs no second source of truth.
  const columns = visible
    ? visible
        .map((key) => COLUMNS.find((c) => c.key === key))
        .filter((c): c is Column => c !== undefined)
    : COLUMNS;
  const templateColumns = columns.map((c) => c.track).join(" ");

  if (error) {
    return <p className="tabular px-3 py-3 text-md text-critical">{error}</p>;
  }

  return (
    // `min-h-40` is a floor: the detail pane below asks for a definite height,
    // and without a floor here a short window gives the grid whatever is left,
    // which was once a single pixel.
    <div className="flex min-h-40 flex-1 flex-col">
    <div
      ref={header}
      // A sibling above the scroller rather than a sticky child inside it.
      // Inside, the transformed rows get their own compositing layer and WebKit
      // paints a sliver of one *over* the header's top edge — visible in a 500k
      // screenshot, and fixed by neither z-index nor `transform-gpu`, because it
      // is compositing order rather than paint order. Outside, the header and
      // the rows share a width and a track template, so they line up with
      // nothing to synchronise — and `px-3` has to match the scroller's, or the
      // headings sit a gutter to the left of their own columns.
      className="grid border-b border-border bg-surface px-3"
      style={{ gridTemplateColumns: templateColumns }}
    >
      {columns.map((column) => {
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
              className={`py-2 text-sm font-medium text-fg-faint/60 ${
                column.numeric ? "text-right" : "text-left"
              }`}
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
            className={`focusable flex items-center gap-1 py-2 text-sm font-medium transition-colors duration-150 ease-state ${
              column.numeric ? "justify-end" : "justify-start"
            } ${active ? "text-fg" : "text-fg-faint hover:text-fg"}`}
          >
            {column.header}
            <span aria-hidden className={active ? "" : "opacity-0"}>
              {direction === "asc" ? "\u2191" : "\u2193"}
            </span>
          </button>
        );
      })}
    </div>

      <div
        ref={scroller}
        tabIndex={0}
        role="grid"
        aria-rowcount={total}
        onKeyDown={(e) => {
          if (total === 0) return;
          // Held keys repeat, and each repeat would otherwise scroll the page
          // as well as the cursor.
          const step = (delta: number) => {
            e.preventDefault();
            const next = Math.min(Math.max(cursor + delta, 0), total - 1);
            setCursor(next);
            virtualizer.scrollToIndex(next, { align: "auto" });
          };
          const rows = Math.max(
            1,
            Math.floor((scroller.current?.clientHeight ?? ROW_HEIGHT) / ROW_HEIGHT) - 1,
          );
          switch (e.key) {
            case "ArrowDown":
              return step(1);
            case "ArrowUp":
              return step(-1);
            case "PageDown":
              return step(rows);
            case "PageUp":
              return step(-rows);
            case "Home":
              return step(-total);
            case "End":
              return step(total);
            case "Enter": {
              const row = rowAt(cursor);
              // A row whose window has not landed yet is not an error and not
              // a no-op worth reporting: the key press simply arrives before
              // the data, and pressing it again works.
              if (row) {
                e.preventDefault();
                onOpen?.(row);
              }
              return;
            }
          }
        }}
        className="focusable min-h-0 flex-1 overflow-x-hidden overflow-y-auto px-3"
      >
      {counted && total === 0 && (
        <div className="flex flex-col items-start gap-2 px-1 py-6">
          <p className="text-md text-fg-muted">{emptyMessage}</p>
          {onClearFilters && (
            <button onClick={onClearFilters} className="btn">
              Clear the filters
            </button>
          )}
        </div>
      )}

      {!counted && waiting && <Skeleton />}

        <div
          style={{ height: virtualizer.getTotalSize(), position: "relative" }}
        >
          {items.map((item) => {
            const row = rowAt(item.index);
            const selected = row !== undefined && row.id === selectedId;
            const focused = item.index === cursor;
            return (
              <div
                key={item.key}
                role="row"
                onClick={() => {
                  setCursor(item.index);
                  if (row) onOpen?.(row);
                }}
                aria-selected={selected}
                className={`grid cursor-default items-center border-b text-md transition-colors duration-150 ease-state ${
                  selected
                    ? "border-accent-line bg-accent-dim"
                    : "border-border/50 hover:bg-raised"
                } ${focused ? "outline -outline-offset-1 outline-accent-line" : ""}`}
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
                  columns.map((column) => (
                    <div
                      key={column.key}
                      className={`min-w-0 pr-3 ${column.numeric ? "text-right" : ""}`}
                    >
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
