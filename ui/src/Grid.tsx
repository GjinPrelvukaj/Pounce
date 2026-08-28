import * as Tooltip from "@radix-ui/react-tooltip";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { MiddleTruncate } from "./MiddleTruncate";
import { urlNotes } from "./urlNotes";
import { statusTone } from "./status";
import { useDelayed } from "./useDelayed";
import {
  queryRows,
  type Filter,
  type ResourceRow,
  type RowView,
  type SitemapUrl,
  type SortColumn,
} from "./engine";

/// A column, as this grid needs one: a width, a heading, and how to draw a
/// cell.
///
/// PLAN.md said TanStack Table here, and it is not carrying its weight:
/// sorting, filtering and pagination are all server-side, and the row model
/// only ever holds the visible window, so the library would contribute column
/// descriptors and a render helper — the twelve lines below — in exchange for
/// an API that changed shape between its last two majors. `react-virtual`
/// stays: measurement, overscan and scroll maths are real work.
/// A column over any row shape.
///
/// The grid is generic because the Images tab is not a filter over `pages` —
/// images live in `resources`, which has no id, no title and no body. Every
/// mechanism above the row shape (windowing, eviction, keyboard, headings) is
/// the same; only the projection differs.
export type AnyColumn<T> = {
  key: string;
  header: string;
  /// A CSS grid track. Fixed for the numeric columns — they are as wide as
  /// their widest value and no wider — and elastic for URL and title, which is
  /// what stops a 1,240px row from needing a horizontal scrollbar next to a
  /// 320px rail.
  track: string;
  /// Right-aligned, because a column of numbers you cannot compare by eye is a
  /// column you have to read one row at a time.
  numeric?: boolean;
  /// Keep the *heading* left-aligned on a numeric column. Only the row number
  /// wants this: a right-aligned "Row" sits against a left-aligned "Status" in
  /// the next track and the two read as one word.
  headerLeft?: boolean;
  /// What this column means, in a sentence. Shown on hover and on keyboard
  /// focus of the heading. "Help and Documentation" scored 2/4 in the design
  /// critique with "no column explanations" named as the reason; a column that
  /// cannot say what it is is a column an agency reader skips.
  help?: string;
  /// The engine's name for this column, when it is one you can sort by. A
  /// column with no `sort` is not sortable — there is no index behind it, and
  /// offering the click would be offering a full scan.
  sort?: SortColumn;
  render: (row: T) => React.ReactNode;
};

/// The page grid's columns, with the key narrowed to what `RowView` can offer.
export type Column = AnyColumn<RowView> & {
  /// A `RowView` field, or a name for a column the grid derives from one —
  /// `titleLength` is `title.length`, and a column that can be computed from
  /// data already on the wire is not worth a byte more of it.
  key:
    | keyof RowView
    | "titleLength"
    | "descriptionLength"
    | "urlLength"
    | "urlParams"
    | "urlNotes"
    | "row";
};


/// The Sitemap tab's columns.
///
/// The comparison is the column: "In the crawl" is the only field here that is
/// not simply a copy of the sitemap, and it is the whole reason the tab
/// exists. A URL the site advertises that no link reaches is an orphan its
/// owner believes is fine.
export const SITEMAP_COLUMNS: AnyColumn<SitemapUrl>[] = [
  {
    key: "row",
    help: "Position in this list.",
    header: "Row",
    track: "3.5rem",
    numeric: true,
    headerLeft: true,
    render: () => null,
  },
  {
    key: "url",
    help: "A URL the site's own sitemap lists.",
    header: "URL in the sitemap",
    track: "minmax(18rem, 3fr)",
    render: (row) => (
      <span className="tabular block text-accent-fg">
        <MiddleTruncate text={row.url} />
      </span>
    ),
  },
  {
    key: "status",
    help: "What the crawl found at this address. “Not reached” means the sitemap lists it but no link on the site leads to it — the page exists as far as the sitemap is concerned and is invisible to anyone following links.",
    header: "In the crawl",
    track: "8rem",
    render: (row) =>
      row.status === null ? (
        <span className="text-warning">Not reached</span>
      ) : (
        <span className={`nums ${statusTone(row.status)}`}>{row.status}</span>
      ),
  },
  {
    key: "source",
    help: "The sitemap document that listed this URL. A site may have several, and this is the file to edit.",
    header: "Listed in",
    track: "minmax(10rem, 1fr)",
    render: (row) => (
      <span className="tabular block truncate text-fg-muted">
        <MiddleTruncate text={row.source} tailLength={16} />
      </span>
    ),
  },
];

/// The Images tab's columns.
///
/// A separate list because an image is not a page. It has a status, a declared
/// length and a declared type, and none of a page's fields — no title, no
/// headings, no word count. The old Images view filtered `pages` for
/// `kind = 'image'`, a kind the crawler has never written since migration 011
/// gave resources their own table, so it showed "no pages match these filters"
/// over a crawl holding 88 images.
export const IMAGE_COLUMNS: AnyColumn<ResourceRow>[] = [
  {
    key: "row",
    help: "Position in this list.",
    header: "Row",
    track: "3.5rem",
    numeric: true,
    headerLeft: true,
    render: () => null,
  },
  {
    key: "status",
    help: "What the server answered when the image was requested. 404 here means a broken image on a page that looks fine.",
    header: "Status",
    track: "5rem",
    render: (row) => (
      <span className={`nums ${statusTone(row.status)}`}>{row.status}</span>
    ),
  },
  {
    key: "url",
    help: "The image's address, as the page referenced it.",
    header: "Image",
    track: "minmax(18rem, 3fr)",
    render: (row) => (
      <span className="tabular block text-accent-fg">
        <MiddleTruncate text={row.url} />
      </span>
    ),
  },
  {
    key: "contentLength",
    help: "Size in kilobytes, as the server declared it. “Not declared” is not zero — a server that says nothing about size is a different report from one that says empty, and it is why an oversized-image check cannot simply assume.",
    header: "Size",
    track: "7rem",
    numeric: true,
    render: (row) =>
      row.contentLength === null ? (
        <span className="text-fg-faint">Not declared</span>
      ) : (
        <span
          className={`nums ${row.contentLength > 200_000 ? "text-warning" : "text-fg-muted"}`}
        >
          {Math.round(row.contentLength / 1024).toLocaleString()} kB
        </span>
      ),
  },
  {
    key: "contentType",
    help: "The type the server declared. An image served as text/html is usually an error page wearing an image's address.",
    header: "Type",
    track: "9rem",
    render: (row) =>
      row.contentType === null ? (
        <span className="text-fg-faint">None</span>
      ) : (
        <span className="block truncate text-fg-muted">{row.contentType}</span>
      ),
  },
  {
    key: "issues",
    help: "Findings about this image. The rules live in the panel on the right; this is how many of them named this file.",
    header: "Findings",
    track: "6rem",
    numeric: true,
    render: (row) => (
      <span className={`nums ${row.issues > 0 ? "text-warning" : "text-fg-faint"}`}>
        {row.issues}
      </span>
    ),
  },
];

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
const ROW_HEIGHT = 38;

/// A text column where absence and emptiness are different findings.
///
/// The store keeps that distinction all the way from the parser, and the grid
/// is where a person finally sees it: a missing `<title>` and `<title></title>`
/// are not the same defect and must not render the same.
function Text({ value }: { value: string | null }) {
  if (value === null) return <span className="text-fg-faint">None</span>;
  if (value === "") return <span className="text-warning">Empty</span>;
  return <span className="block truncate">{value}</span>;
}

/// A character count, amber past the length search results start cutting.
///
/// The threshold is advice, not a rule — `title.too-long` is the rule, and it
/// is in the findings panel. This is the same fact where you are already
/// looking.
function Length({ value, over }: { value: string | null; over: number }) {
  if (value === null) return <span className="text-fg-faint" />;
  return (
    <span className={`nums ${value.length > over ? "text-warning" : "text-fg-muted"}`}>
      {value.length}
    </span>
  );
}

export const COLUMNS: Column[] = [
  {
    key: "row",
    help: "Position in the current view. Changes when you sort or filter, because it is a place in this list rather than an identity.",
    header: "Row",
    // Wider than the digits need: a right-aligned "Row" against a left-aligned
    // "Status" in the next track reads as one word without the slack.
    track: "4.75rem",
    headerLeft: true,
    numeric: true,
    // The virtualiser's index, not a stored column: it is a position in *this*
    // sorted, filtered result, which is exactly what a row number means.
    render: () => null,
  },
  {
    key: "status",
    help: "The HTTP code the server answered with. 2xx worked, 3xx redirected, 4xx was not found, 5xx failed.",
    sort: "status",
    header: "Status",
    track: "4.5rem",
    render: (row) => (
      <span className={`nums ${statusTone(row.status)}`}>{row.status}</span>
    ),
  },
  {
    key: "url",
    help: "The address, shortened from the middle so the end of the path stays readable.",
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
    help: "The page\u2019s title tag. \u201cNone\u201d means the tag is absent; \u201cEmpty\u201d means it is present with nothing in it, which are different defects.",
    sort: "title",
    header: "Title",
    track: "minmax(12rem, 2fr)",
    render: (row) => <Text value={row.title} />,
  },
  {
    key: "titleLength",
    help: "Characters in the title. Amber past 60, where Google usually truncates it in results.",
    header: "Title length",
    track: "6.5rem",
    numeric: true,
    // Computed here rather than stored: it is `title.length`, and a column the
    // grid can derive from a column it already has is not worth a byte on the
    // wire or a migration.
    render: (row) => <Length value={row.title} over={60} />,
  },
  {
    key: "metaDescription",
    help: "The meta description search results usually show. \u201cNone\u201d means absent, \u201cEmpty\u201d means present but blank.",
    header: "Meta description",
    track: "minmax(14rem, 3fr)",
    // Sortable so duplicates sit next to each other: a finding that says
    // "more than one page uses this description" is unreadable until they do.
    sort: "metaDescription",
    render: (row) => <Text value={row.metaDescription} />,
  },
  {
    key: "descriptionLength",
    help: "Characters in the description. Amber past 155, where results usually cut it off.",
    header: "Description length",
    track: "8rem",
    numeric: true,
    render: (row) => <Length value={row.metaDescription} over={155} />,
  },
  {
    key: "urlLength",
    help: "Characters in the address. Amber past 115 — long URLs are not a ranking problem, they are a sharing problem: they wrap in emails and get truncated in search results.",
    header: "URL length",
    track: "6.5rem",
    numeric: true,
    render: (row) => (
      <span
        className={`nums ${row.url.length > 115 ? "text-warning" : "text-fg-muted"}`}
      >
        {row.url.length}
      </span>
    ),
  },
  {
    key: "urlParams",
    help: "Everything after the “?”. Parameters are how one page ends up at many addresses, which is the usual cause of duplicate content.",
    header: "Parameters",
    track: "minmax(8rem, 1fr)",
    render: (row) => {
      const q = row.url.indexOf("?");
      return q === -1 ? (
        <span className="text-fg-faint">None</span>
      ) : (
        <span className="tabular block truncate text-warning">
          {row.url.slice(q + 1)}
        </span>
      );
    },
  },
  {
    key: "urlNotes",
    help: "What is unusual about this address: capitals, underscores, encoded characters, or a path more than five levels deep. None of these is a defect on its own; together they are what makes a URL hard to read and hard to share.",
    header: "Notes",
    track: "minmax(9rem, 1fr)",
    render: (row) => {
      const notes = urlNotes(row.url);
      return notes.length === 0 ? (
        <span className="text-fg-faint">Clean</span>
      ) : (
        <span className="block truncate text-fg-muted">{notes.join(", ")}</span>
      );
    },
  },
  {
    key: "h1",
    help: "The page\u2019s first H1 \u2014 the headline a reader sees, and the one a search engine weighs most. \u201cNone\u201d means the page has no H1 at all.",
    header: "H1",
    track: "minmax(12rem, 2fr)",
    render: (row) => <Text value={row.h1} />,
  },
  {
    key: "h1Count",
    help: "How many H1s the page has. One is the answer. Zero leaves the page without a headline; more than one splits it.",
    header: "H1s",
    track: "4.5rem",
    numeric: true,
    render: (row) => (
      <span className={`nums ${row.h1Count === 1 ? "" : "text-warning"}`}>
        {row.h1Count}
      </span>
    ),
  },
  {
    key: "h2",
    help: "The page\u2019s first H2 \u2014 the first subheading under the headline.",
    header: "H2",
    track: "minmax(12rem, 2fr)",
    render: (row) => <Text value={row.h2} />,
  },
  {
    key: "h2Count",
    help: "How many H2s the page has. No single right answer; zero on a long page usually means one wall of text.",
    header: "H2s",
    track: "4.5rem",
    numeric: true,
    render: (row) => <span className="nums">{row.h2Count}</span>,
  },
  {
    key: "canonical",
    help: "The URL this page names as the version that should rank. Pointing somewhere else means this page is not the one you will see in results.",
    header: "Canonical",
    track: "minmax(12rem, 2fr)",
    render: (row) =>
      row.canonical === null ? (
        <span className="text-fg-faint">None</span>
      ) : (
        <span className="tabular block text-fg-muted">
          <MiddleTruncate text={row.canonical} tailLength={20} />
        </span>
      ),
  },
  {
    key: "elapsedMs",
    help: "How long the whole response took, from request to last byte.",
    sort: "elapsedMs",
    header: "Response",
    track: "6rem",
    numeric: true,
    render: (row) => (
      <span className="nums text-fg-muted">
        {row.elapsedMs.toLocaleString()} ms
      </span>
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
        <span className="text-warning">No (noindex)</span>
      ) : (
        <span className="text-fg-muted">Yes</span>
      ),
  },
  {
    key: "wordCount",
    help: "Words in the rendered text, excluding markup, script and style.",
    sort: "wordCount",
    header: "Words",
    track: "5.5rem",
    numeric: true,
    render: (row) => (
      <span className="nums text-fg-muted">
        {row.wordCount.toLocaleString()}
      </span>
    ),
  },
  {
    key: "depth",
    help: "Clicks from the home page along the shortest path the crawler found.",
    sort: "depth",
    header: "Depth",
    track: "4.5rem",
    numeric: true,
    render: (row) => <span className="nums text-fg-muted">{row.depth}</span>,
  },
  {
    key: "size",
    help: "Bytes actually downloaded, after any truncation.",
    sort: "size",
    header: "Bytes",
    track: "6rem",
    numeric: true,
    render: (row) => (
      <span className="nums text-fg-muted">{row.size.toLocaleString()}</span>
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

/// A column heading that can say what it means.
///
/// Radix handles the delay, the collision flipping and — the part that matters
/// — showing on keyboard focus as well as hover, so the explanation is not
/// mouse-only.
function Help({ text, children }: { text?: string; children: React.ReactNode }) {
  if (!text) return <>{children}</>;
  return (
    <Tooltip.Root>
      <Tooltip.Trigger asChild>
        <span className="cursor-help">{children}</span>
      </Tooltip.Trigger>
      <Tooltip.Portal>
        <Tooltip.Content className="tip" sideOffset={6} collisionPadding={8}>
          {text}
        </Tooltip.Content>
      </Tooltip.Portal>
    </Tooltip.Root>
  );
}

export function Grid<T extends object>({
  filters,
  visible,
  sort,
  direction,
  supportedSorts,
  onSort,
  refreshKey = 0,
  emptyMessage = "No pages match this filter.",
  /// False before any crawl is open. The grid still draws its column headings,
  /// because those headings *are* the structure a new user learns the tool
  /// from, and it simply does not query.
  enabled = true,
  emptyContent,
  onClearFilters,
  selectedId = null,
  onOpen,
  registerFocus,
  onTotal,
  allColumns,
  source,
  sourceKey,
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
  enabled?: boolean;
  emptyContent?: React.ReactNode;
  /// Offered beside the empty message. An empty grid with no way out of it is
  /// the state people close the app in.
  onClearFilters?: () => void;
  /// The row the detail pane is showing, if any.
  selectedId?: number | null;
  onOpen?: (row: T) => void;
  /// Hands the caller a way to put the keyboard back here — used when the
  /// detail pane closes.
  registerFocus?: (focus: () => void) => void;
  onTotal?: (total: number) => void;
  /// The columns to draw. Defaults to the page grid's, which is what every
  /// view but Images uses.
  allColumns?: AnyColumn<T>[];
  /// Where rows come from. Defaults to `query_rows` over `pages`; the Images
  /// tab supplies the resources window instead, because images are not pages
  /// and never were — see migration 011.
  source?: (offset: number, limit: number) => Promise<{ rows: T[]; total: number }>;
  /// Identifies the source in the cache key. Two views with the same filters
  /// but different sources are different datasets, and row 40,000 of one has
  /// nothing to do with row 40,000 of the other.
  sourceKey?: string;
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
  const windows = useRef(new Map<number, T[]>());
  const header = useRef<HTMLDivElement>(null);
  const inflight = useRef(new Set<number>());
  const scroller = useRef<HTMLDivElement>(null);

  const key = useMemo(
    () => JSON.stringify({ filters, sort, direction, sourceKey }),
    [filters, sort, direction, sourceKey],
  );

  // The default source is the page grid's. Wrapped rather than branched at
  // each call site, so the windowing below has one path through it.
  const fetchWindow = useMemo(
    () =>
      source ??
      ((offset: number, limit: number) =>
        queryRows({ filters, sort, direction, offset, limit }) as unknown as Promise<{
          rows: T[];
          total: number;
        }>),
    [source, filters, sort, direction],
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
    if (!enabled) {
      setTotal(0);
      return;
    }
    if (changed) {
      scroller.current?.scrollTo({ top: 0 });
      setCursor(0);
    }
    // Window 0 whether or not it is on screen: this is the call that carries
    // `total`, and during a crawl the total is the number that is moving.
    fetchWindow(0, WINDOW)
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
  }, [key, refreshKey, enabled]);

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
      fetchWindow(w * WINDOW, WINDOW)
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

  const rowAt = (index: number): T | undefined =>
    windows.current.get(Math.floor(index / WINDOW))?.[index % WINDOW];

  // Ordered by the picker's list rather than by the column list itself, so a
  // future reordering needs no second source of truth.
  const available = allColumns ?? (COLUMNS as unknown as AnyColumn<T>[]);
  const columns = visible
    ? visible
        .map((key) => available.find((c) => c.key === key))
        .filter((c): c is AnyColumn<T> => c !== undefined)
    : available;
  const templateColumns = columns.map((c) => c.track).join(" ");

  if (error) {
    return <p className="nums px-3 py-3 text-sm text-critical">{error}</p>;
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
                  ? `Sorting by ${column.header.toLowerCase()} is not offered with the filters applied. No index serves that pair, and the query would scan the whole crawl.`
                  : undefined
              }
              // `pr-3` matches the body cells; without it a right-aligned
              // heading sits flush against the next column's left-aligned one.
              className={`py-2 pr-3 text-xs font-semibold tracking-[0.06em] text-fg-faint/60 uppercase ${
                column.numeric && !column.headerLeft ? "text-right" : "text-left"
              }`}
            >
              <Help text={column.help}>{column.header}</Help>
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
            className={`focusable flex items-center gap-1 py-2 pr-3 text-xs font-semibold tracking-[0.06em] uppercase transition-colors duration-150 ease-state ${
              column.numeric && !column.headerLeft
                ? "justify-end"
                : "justify-start"
            } ${active ? "text-fg" : "text-fg-faint hover:text-fg"}`}
          >
            <Help text={column.help}>{column.header}</Help>
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
      {!enabled && (
        <div className="flex flex-1 flex-col items-center justify-center gap-4 py-10">
          <p className="text-md text-fg-faint">No data</p>
          {emptyContent}
        </div>
      )}

      {enabled && counted && total === 0 && (
        <div className="flex flex-col items-start gap-2 px-1 py-6">
          <p className="text-sm text-fg-muted">{emptyMessage}</p>
          {onClearFilters && (
            <button onClick={onClearFilters} className="btn">
              Clear the filters
            </button>
          )}
        </div>
      )}

      {enabled && !counted && waiting && <Skeleton />}

        <div
          style={{ height: virtualizer.getTotalSize(), position: "relative" }}
        >
          {items.map((item) => {
            const row = rowAt(item.index);
            // Rows that have no id are never selected, which is right: a
            // resource has no detail pane to be selected *into*.
            const selected =
              row !== undefined &&
              selectedId !== null &&
              (row as { id?: number }).id === selectedId;
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
                className={`grid cursor-default items-center border-b text-sm transition-colors duration-150 ease-state ${
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
                      {column.key === "row" ? (
                        <span className="nums text-fg-faint">
                          {(item.index + 1).toLocaleString()}
                        </span>
                      ) : (
                        column.render(row)
                      )}
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
