import { useEffect, useMemo, useRef, useState } from "react";
import {
  closeCrawl,
  currentCrawl,
  engineInfo,
  issueOverview,
  listRules,
  openCrawl,
  supportedSorts,
  type ApiError,
  type CrawlHandle,
  type EngineInfo,
  type IssueOverview,
  type ProgressEvent,
  type RuleInfo,
  type SortColumn,
} from "./engine";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Grid } from "./Grid";
import {
  FilterBar,
  NO_FILTERS,
  toFilters,
  type FilterState,
} from "./Filters";
import { IssueList, selectionFilters, type IssueSelection } from "./Issues";
import { NewCrawl } from "./NewCrawl";
import { ago, basename, forget, recents, remember, type Recent } from "./recents";
import {
  resolve,
  setChoice,
  storedChoice,
  watchSystem,
  type ThemeChoice,
} from "./theme";

const THEMES: ThemeChoice[] = ["light", "dark", "system"];

/// Reads a rejected command back as the typed failure it is. Tauri hands
/// command errors through as the serialised payload, so this is a cast with a
/// guard rather than parsing.
function asApiError(e: unknown): ApiError | null {
  return typeof e === "object" && e !== null && "kind" in e
    ? (e as ApiError)
    : null;
}

function describe(e: unknown): string {
  const api = asApiError(e);
  if (!api) return String(e);
  switch (api.kind) {
    case "noCrawlOpen":
      return "no crawl open";
    case "unknownRule":
      return `no such rule: ${api.rule}`;
    case "unsupportedPair":
      return `sorting by ${api.sort} is not offered with a ${api.filter} filter`;
    case "crawl":
      return api.message;
    case "store":
      return api.message;
  }
}

export default function App() {
  const [info, setInfo] = useState<EngineInfo | null>(null);
  // Bumped when a crawl finishes, which remounts the pane so it picks up the
  // file the engine just left open.
  const [openedAt, setOpenedAt] = useState(0);
  // Fetched once. Thirty rules is a few kilobytes of prose, and it is the same
  // prose for every file this build opens.
  const [rules, setRules] = useState<Map<string, RuleInfo>>(new Map());
  // The crawl in flight and the file it is filling. Held here because two
  // children need it: the form draws the progress, and the results pane opens
  // that file *while* it is being written.
  const [live, setLive] = useState<Live | null>(null);
  const [choice, setChoiceState] = useState<ThemeChoice>(storedChoice);
  const [resolved, setResolved] = useState(() => resolve(storedChoice()));

  useEffect(() => {
    engineInfo().then(setInfo).catch(() => setInfo(null));
  }, []);
  useEffect(() => {
    listRules()
      .then((all) => setRules(new Map(all.map((r) => [r.id, r]))))
      .catch(() => {});
  }, []);
  useEffect(() => watchSystem(setResolved), []);

  return (
    <div className="flex h-full flex-col bg-canvas text-fg">
      <header className="flex items-center justify-between border-b border-border bg-surface px-4 py-2.5">
        <div className="flex items-baseline gap-2">
          <span className="text-xl font-semibold tracking-tight">Pounce</span>
          <span className="tabular text-xs text-fg-faint">
            {info
              ? `engine ${info.version} · schema ${info.schemaVersion} · ${info.rules} rules`
              : "no engine — run the desktop shell"}
          </span>
        </div>
        <div className="flex items-center gap-2">
          <span className="tabular text-xs text-fg-faint">
            {choice === "system" ? `system · ${resolved}` : resolved}
          </span>
          <div className="flex rounded-md border border-border bg-raised p-0.5">
            {THEMES.map((t) => (
              <button
                key={t}
                onClick={() => {
                  setChoiceState(t);
                  setResolved(setChoice(t));
                }}
                aria-pressed={choice === t}
                className={`rounded-sm px-2 py-1 text-sm capitalize transition-colors duration-150 ease-state ${
                  choice === t
                    ? "bg-accent text-on-accent"
                    : "text-fg-muted hover:text-fg"
                }`}
              >
                {t}
              </button>
            ))}
          </div>
        </div>
      </header>
      <NewCrawl
        onDone={() => {
          setLive(null);
          setOpenedAt(Date.now());
        }}
        onProgress={(p, output) =>
          setLive(p && output ? { path: output, progress: p } : null)
        }
      />
      <CrawlPane key={openedAt} live={live} rules={rules} />
    </div>
  );
}

/// A crawl in flight, as the results pane needs it: the file being written and
/// the last tick that described it.
type Live = { path: string; progress: ProgressEvent };

/// Scaffolding for the real screens: a path field stands in for T4.8's file
/// dialog, and the row list stands in for T4.9's virtualised grid. What it is
/// here to prove is the boundary — a `.pounce` file on disk becomes windows of
/// rows in the window, without the dataset crossing it.
function CrawlPane({
  live,
  rules,
}: {
  live: Live | null;
  rules: Map<string, RuleInfo>;
}) {
  const [handle, setHandle] = useState<CrawlHandle | null>(null);
  const [recent, setRecent] = useState<Recent[]>(recents);
  const [overview, setOverview] = useState<IssueOverview | null>(null);
  const [total, setTotal] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // Which issue the grid is filtered to. Held here rather than in the grid
  // because the issue list and the grid are two views of one selection.
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

  const filters = useMemo(
    () => [...selectionFilters(selection), ...toFilters(bar)],
    [selection, bar],
  );
  const filterKey = JSON.stringify(filters);

  // `url` is the fallback because it is the one column supported against every
  // filter shape this build has — a substring filter is *only* offered with it.
  useEffect(() => {
    if (!handle) return;
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
  }, [filterKey, handle]);

  // One overview query at a time. The count is a `GROUP BY` over every issue
  // in the file, so on a large crawl it can take longer than the second
  // between ticks — and queuing them would put the reader in the writer's way
  // rather than out of it.
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

  // Results while the crawl runs. The store is disk-backed from the first row
  // and WAL gives one writer and many readers, so the only thing that used to
  // stand between a running crawl and a filling grid was that nobody opened
  // the file. Ticked once a second rather than at the progress rate: 10 Hz of
  // `count(*)` over a growing table is the reader competing with the writer.
  const liveSecond = live ? Math.floor(live.progress.elapsedMs / 1000) : 0;
  const livePath = live && live.progress.written > 0 ? live.path : null;
  useEffect(() => {
    if (!livePath) return;
    if (handle?.path === livePath) {
      setRefreshKey((k) => k + 1);
      refreshOverview();
    } else {
      void load(livePath);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [livePath, liveSecond]);

  // A file handed to the process on the command line is already open in the
  // engine by the time the window exists; the UI just has to catch up with it.
  useEffect(() => {
    currentCrawl()
      .then((current) => {
        if (current) void load(current);
      })
      .catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /// The native dialog, filtered to the one extension this app reads.
  async function pick() {
    const chosen = await openDialog({
      multiple: false,
      directory: false,
      filters: [{ name: "Pounce crawl", extensions: ["pounce"] }],
    });
    if (typeof chosen === "string") await load(chosen);
  }

  async function load(target: string) {
    setBusy(true);
    setError(null);
    try {
      const opened = await openCrawl(target);
      setHandle(opened);
      setSelection(null);
      setBar(NO_FILTERS);
      setRecent(remember(opened.path, opened.pages));
      setOverview(await issueOverview());
    } catch (e) {
      setError(describe(e));
      setHandle(null);
      setOverview(null);
    } finally {
      setBusy(false);
    }
  }

  async function close() {
    await closeCrawl().catch(() => {});
    setHandle(null);
    setOverview(null);
    setSelection(null);
    setBar(NO_FILTERS);
    setTotal(0);
  }

  return (
    <main className="flex min-h-0 flex-1 flex-col gap-3 p-4">
      <div className="flex flex-wrap items-center gap-2">
        <button
          onClick={() => void pick()}
          disabled={busy}
          className="rounded-sm bg-accent px-2.5 py-1.5 text-sm text-on-accent transition-colors duration-150 ease-state disabled:opacity-60"
        >
          {busy ? "Opening…" : "Open crawl…"}
        </button>
        {handle && (
          <>
            <span className="tabular text-xs text-fg-muted" title={handle.path}>
              {basename(handle.path)}
            </span>
            <button
              onClick={() => void close()}
              className="rounded-sm border border-border px-2.5 py-1.5 text-sm text-fg-muted transition-colors duration-150 ease-state hover:text-fg"
            >
              Close
            </button>
          </>
        )}
        {error && <span className="tabular text-sm text-critical">{error}</span>}
      </div>

      {!handle && recent.length > 0 && (
        <div className="flex flex-col gap-1">
          <span className="text-sm text-fg-faint">Recent</span>
          <div className="flex flex-wrap gap-2">
            {recent.map((r) => (
              <span
                key={r.path}
                className="flex items-center gap-2 rounded-sm border border-border bg-raised px-2 py-1"
              >
                <button
                  onClick={() => void load(r.path)}
                  title={r.path}
                  className="tabular text-sm text-accent-fg hover:underline"
                >
                  {basename(r.path)}
                </button>
                <span className="tabular text-xs text-fg-faint">
                  {r.pages.toLocaleString()} pages · {ago(r.openedAt)}
                </span>
                <button
                  onClick={() => setRecent(forget(r.path))}
                  aria-label={`Remove ${basename(r.path)} from recent crawls`}
                  className="text-xs text-fg-faint hover:text-critical"
                >
                  ✕
                </button>
              </span>
            ))}
          </div>
        </div>
      )}

      {handle && (
        <p className="tabular text-sm text-fg-muted">
          {(live ? live.progress.written : handle.pages).toLocaleString()} pages
          {live ? " so far" : ""} · schema {handle.schemaVersion}
          {overview
            ? ` · ${overview.totalIssues.toLocaleString()} issues on ${overview.urlsWithIssues.toLocaleString()} URLs`
            : ""}
        </p>
      )}

      {overview && (
        <IssueList
          overview={overview}
          rules={rules}
          selection={selection}
          live={live !== null}
          onSelect={setSelection}
        />
      )}

      {handle && <FilterBar value={bar} onChange={setBar} />}

      {handle && filters.length > 0 && (
        <div className="flex items-center gap-2">
          <span className="tabular text-sm text-fg">
            Showing {total.toLocaleString()} of{" "}
            {(live ? live.progress.written : handle.pages).toLocaleString()}{" "}
            pages
            {selection !== null &&
              ` — ${
                selection === "*"
                  ? "every page with something to fix"
                  : (rules.get(selection)?.description ?? selection)
              }`}
          </span>
          {selection !== null && (
            <button
              onClick={() => setSelection(null)}
              className="rounded-sm border border-border px-2 py-0.5 text-sm text-fg-muted transition-colors duration-150 ease-state hover:text-fg focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent"
            >
              Clear finding
            </button>
          )}
        </div>
      )}

      {handle && (
        <Grid
          filters={filters}
          sort={sort}
          direction={direction}
          supportedSorts={sorts}
          onSort={(column) => {
            // Clicking the column already sorted reverses it; clicking another
            // starts that one ascending, which is what every table does and
            // the only behaviour nobody has to be told about.
            if (column === sort) {
              setDirection((d) => (d === "asc" ? "desc" : "asc"));
            } else {
              setSort(column);
              setDirection("asc");
            }
          }}
          refreshKey={refreshKey}
          emptyMessage={
            live
              ? "No rows yet — pages reach the file 500 at a time, and the first batch has not landed."
              : filters.length === 0
                ? "This crawl has no pages."
                : "No pages match these filters."
          }
          onTotal={(t) => setTotal(t)}
        />
      )}
    </main>
  );
}
