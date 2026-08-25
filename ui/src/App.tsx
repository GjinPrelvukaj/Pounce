import { useEffect, useState } from "react";
import {
  closeCrawl,
  currentCrawl,
  engineInfo,
  issueOverview,
  openCrawl,
  type ApiError,
  type CrawlHandle,
  type EngineInfo,
  type IssueOverview,
} from "./engine";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Grid } from "./Grid";
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
  const [choice, setChoiceState] = useState<ThemeChoice>(storedChoice);
  const [resolved, setResolved] = useState(() => resolve(storedChoice()));

  useEffect(() => {
    engineInfo().then(setInfo).catch(() => setInfo(null));
  }, []);
  useEffect(() => watchSystem(setResolved), []);

  return (
    <div className="flex h-full flex-col bg-canvas text-fg">
      <header className="flex items-center justify-between border-b border-border bg-surface px-4 py-2.5">
        <div className="flex items-baseline gap-2">
          <span className="text-lg font-semibold tracking-tight">Pounce</span>
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
                className={`rounded-sm px-2 py-1 text-xs capitalize transition-colors duration-150 ease-state ${
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
      <NewCrawl onDone={() => setOpenedAt(Date.now())} />
      <CrawlPane key={openedAt} />
    </div>
  );
}

/// Scaffolding for the real screens: a path field stands in for T4.8's file
/// dialog, and the row list stands in for T4.9's virtualised grid. What it is
/// here to prove is the boundary — a `.pounce` file on disk becomes windows of
/// rows in the window, without the dataset crossing it.
function CrawlPane() {
  const [handle, setHandle] = useState<CrawlHandle | null>(null);
  const [recent, setRecent] = useState<Recent[]>(recents);
  const [overview, setOverview] = useState<IssueOverview | null>(null);
  const [total, setTotal] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // Which issue the grid is filtered to. Held here rather than in the grid
  // because the issue list and the grid are two views of one selection.
  const [selection, setSelection] = useState<IssueSelection>(null);

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
    setTotal(0);
  }

  return (
    <main className="flex min-h-0 flex-1 flex-col gap-3 p-4">
      <div className="flex flex-wrap items-center gap-2">
        <button
          onClick={() => void pick()}
          disabled={busy}
          className="rounded-sm bg-accent px-2.5 py-1 text-xs text-on-accent transition-colors duration-150 ease-state disabled:opacity-60"
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
              className="rounded-sm border border-border px-2.5 py-1 text-xs text-fg-muted hover:text-fg"
            >
              Close
            </button>
          </>
        )}
        {error && <span className="tabular text-xs text-critical">{error}</span>}
      </div>

      {!handle && recent.length > 0 && (
        <div className="flex flex-col gap-1">
          <span className="text-xs text-fg-faint">Recent</span>
          <div className="flex flex-wrap gap-2">
            {recent.map((r) => (
              <span
                key={r.path}
                className="flex items-center gap-2 rounded-sm border border-border bg-raised px-2 py-1"
              >
                <button
                  onClick={() => void load(r.path)}
                  title={r.path}
                  className="tabular text-xs text-accent-fg hover:underline"
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
        <p className="tabular text-xs text-fg-muted">
          {handle.pages.toLocaleString()} pages · schema {handle.schemaVersion}
          {overview
            ? ` · ${overview.totalIssues.toLocaleString()} issues on ${overview.urlsWithIssues.toLocaleString()} URLs`
            : ""}
        </p>
      )}

      {overview && (
        <IssueList
          overview={overview}
          selection={selection}
          onSelect={setSelection}
        />
      )}

      {handle && selection !== null && (
        <div className="flex items-center gap-2">
          <span className="tabular text-xs text-fg">
            Showing {total.toLocaleString()} of{" "}
            {handle.pages.toLocaleString()} pages —{" "}
            {selection === "*" ? "any issue" : selection}
          </span>
          <button
            onClick={() => setSelection(null)}
            className="rounded-sm border border-border px-2 py-0.5 text-xs text-fg-muted transition-colors duration-150 ease-state hover:text-fg focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent"
          >
            Clear filter
          </button>
        </div>
      )}

      {handle && (
        <Grid
          filters={selectionFilters(selection)}
          sort="url"
          direction="asc"
          onTotal={(t) => setTotal(t)}
        />
      )}
    </main>
  );
}
