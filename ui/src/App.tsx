import { useEffect, useState } from "react";
import {
  closeCrawl,
  currentCrawl,
  engineInfo,
  issueOverview,
  openCrawl,
  queryRows,
  type ApiError,
  type CrawlHandle,
  type EngineInfo,
  type IssueOverview,
  type Page,
} from "./engine";
import { NewCrawl } from "./NewCrawl";
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
  const [path, setPath] = useState("");
  const [handle, setHandle] = useState<CrawlHandle | null>(null);
  const [page, setPage] = useState<Page | null>(null);
  const [overview, setOverview] = useState<IssueOverview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // A file handed to the process on the command line is already open in the
  // engine by the time the window exists; the UI just has to catch up with it.
  useEffect(() => {
    currentCrawl()
      .then((current) => {
        if (current) {
          setPath(current);
          void load(current);
        }
      })
      .catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function load(target: string) {
    setBusy(true);
    setError(null);
    try {
      const opened = await openCrawl(target);
      setHandle(opened);
      setPage(
        await queryRows({
          filters: [],
          sort: "url",
          direction: "asc",
          offset: 0,
          limit: 50,
        }),
      );
      setOverview(await issueOverview());
    } catch (e) {
      setError(describe(e));
      setHandle(null);
      setPage(null);
      setOverview(null);
    } finally {
      setBusy(false);
    }
  }

  async function open() {
    await load(path);
  }

  async function close() {
    await closeCrawl().catch(() => {});
    setHandle(null);
    setPage(null);
    setOverview(null);
  }

  return (
    <main className="flex flex-1 flex-col gap-3 overflow-auto p-4">
      <div className="flex items-center gap-2">
        <input
          value={path}
          onChange={(e) => setPath(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && void open()}
          spellCheck={false}
          placeholder="/path/to/crawl.pounce"
          className="tabular w-96 rounded-sm border border-border bg-raised px-2 py-1 text-xs text-fg outline-none placeholder:text-fg-faint focus:border-accent-line"
        />
        <button
          onClick={() => void open()}
          disabled={busy}
          className="rounded-sm bg-accent px-2.5 py-1 text-xs text-on-accent transition-colors duration-150 ease-state disabled:opacity-60"
        >
          {busy ? "Opening…" : "Open"}
        </button>
        {handle && (
          <button
            onClick={() => void close()}
            className="rounded-sm border border-border px-2.5 py-1 text-xs text-fg-muted hover:text-fg"
          >
            Close
          </button>
        )}
        {error && <span className="tabular text-xs text-critical">{error}</span>}
      </div>

      {handle && page && (
        <p className="tabular text-xs text-fg-muted">
          {handle.pages.toLocaleString()} pages · schema {handle.schemaVersion} ·
          showing {page.rows.length} of {page.total.toLocaleString()} matching
          {overview
            ? ` · ${overview.totalIssues.toLocaleString()} issues on ${overview.urlsWithIssues.toLocaleString()} URLs`
            : ""}
        </p>
      )}

      {overview && overview.byRule.length > 0 && (
        <div className="flex flex-wrap gap-2">
          {overview.byRule.slice(0, 6).map((r) => (
            <span
              key={`${r.ruleId}-${r.severity}`}
              className="tabular rounded-sm border border-border bg-raised px-2 py-1 text-xs text-fg-muted"
            >
              {r.ruleId} · {r.issues.toLocaleString()}
            </span>
          ))}
        </div>
      )}

      {page && (
        <table className="w-full border-collapse text-left">
          <thead>
            <tr className="border-b border-border text-xs text-fg-faint">
              <th className="py-1 pr-3 font-medium">Status</th>
              <th className="py-1 pr-3 font-medium">URL</th>
              <th className="py-1 pr-3 font-medium">Words</th>
              <th className="py-1 font-medium">Title</th>
            </tr>
          </thead>
          <tbody>
            {page.rows.map((row) => (
              <tr key={row.id} className="border-b border-border/60">
                <td
                  className={`tabular py-1 pr-3 text-xs ${
                    row.status >= 400 ? "text-critical" : "text-pass"
                  }`}
                >
                  {row.status}
                </td>
                <td className="tabular py-1 pr-3 text-xs text-accent-fg">
                  {row.url}
                </td>
                <td className="tabular py-1 pr-3 text-xs text-fg-muted">
                  {row.wordCount.toLocaleString()}
                </td>
                <td className="truncate py-1 text-xs text-fg-muted">
                  {row.title ?? <span className="text-fg-faint">— none</span>}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </main>
  );
}
