import { useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { About } from "./About";
import { NewCrawl } from "./NewCrawl";
import { Results, type Live } from "./Results";
import { RunStrip } from "./RunStrip";
import { Welcome } from "./Welcome";
import {
  closeCrawl,
  currentCrawl,
  listRules,
  openCrawl,
  startCrawl,
  startupError,
  type ApiError,
  type CrawlHandle,
  type CrawlSettings,
  type RuleInfo,
} from "./engine";
import { basename, forget, recents, remember, type Recent } from "./recents";
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

/// A failure and the way out of it.
///
/// "output already exists: /tmp/x.pounce" is accurate and unhelpful: it names
/// what went wrong and leaves the user to invent the next filename. Where the
/// engine can say what to do instead, it does, and the interface offers it as
/// a button.
export type Failure = {
  message: string;
  fix?: { label: string; apply: (s: CrawlSettings) => CrawlSettings };
};

function describe(e: unknown): Failure {
  const api = asApiError(e);
  if (!api) return { message: String(e) };
  switch (api.kind) {
    case "noCrawlOpen":
      return { message: "No crawl is open." };
    case "unknownRule":
      return { message: `This build has no rule called ${api.rule}.` };
    case "unsupportedPair":
      return {
        message: `Sorting by ${api.sort} is not offered with a ${api.filter} filter — no index serves that pair.`,
      };
    case "outputExists":
      return {
        message: `${api.path} already exists, and Pounce will not write over a crawl.`,
        fix: {
          label: `Save as ${basename(api.suggestion)} instead`,
          apply: (s) => ({ ...s, output: api.suggestion }),
        },
      };
    case "badSeed":
      return {
        message: `${api.input} is not a URL Pounce can crawl — ${api.message}.`,
        fix: api.suggestion
          ? {
              label: `Try ${api.suggestion}`,
              apply: (s) => ({ ...s, seed: api.suggestion! }),
            }
          : undefined,
      };
    case "notACrawl":
      return {
        message: `${basename(api.path)} is a database, but not a Pounce crawl. Choose a .pounce file, or start a new crawl.`,
      };
    case "missing":
      return {
        message: `${basename(api.path)} is not there any more — it may have been moved or deleted.`,
      };
    case "export":
      return { message: `The export did not happen — ${api.message}.` };
    case "crawl":
      return { message: api.message };
    case "store":
      return { message: api.message };
  }
}

/// Setup, running and results are three states of one task, and used to be
/// three sections of one scrolling page. The screen is derived rather than
/// stored wherever it can be — a crawl in flight *is* the running state, and
/// an open file *is* the results state.
type Screen = "welcome" | "setup" | "results";

export default function App() {
  const [handle, setHandle] = useState<CrawlHandle | null>(null);
  const [live, setLive] = useState<Live | null>(null);
  const [recent, setRecent] = useState<Recent[]>(recents);
  const [rules, setRules] = useState<Map<string, RuleInfo>>(new Map());
  const [error, setError] = useState<Failure | null>(null);
  const [busy, setBusy] = useState(false);
  const [setup, setSetup] = useState(false);
  // A correction the user accepted, handed to the form to apply to its fields.
  // Held here because the failure it came from is held here.
  const [pending, setPending] = useState<Failure["fix"] | null>(null);
  const [choice, setChoiceState] = useState<ThemeChoice>(storedChoice);
  const [resolved, setResolved] = useState(() => resolve(storedChoice()));
  // Bumped when a file is opened or a crawl finishes, so the results screen
  // starts over rather than carrying the previous crawl's filters and cursor.
  const [epoch, setEpoch] = useState(0);

  const screen: Screen = setup ? "setup" : handle ? "results" : "welcome";

  useEffect(() => {
    // Thirty rules is a few kilobytes of prose, and it is the same prose for
    // every file this build opens.
    listRules()
      .then((all) => setRules(new Map(all.map((r) => [r.id, r]))))
      .catch(() => {});
    // A file handed to the process on the command line is already open in the
    // engine by the time the window exists; the UI just has to catch up.
    currentCrawl()
      .then((current) => {
        if (current) {
          void open(current);
          return;
        }
        // No file open, so either none was given or it failed. "Open With" is
        // a door people arrive through, and the welcome screen is where they
        // land when it does not work.
        void startupError().then((e) => {
          if (!e) return;
          setError(describe(e));
          // The same rule as the Open path: a recent whose file has gone stops
          // being a recent, however the app was asked to open it.
          if (e.kind === "missing") setRecent(forget(e.path));
        });
      })
      .catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  useEffect(() => watchSystem(setResolved), []);

  async function open(path?: string) {
    let target = path;
    if (target === undefined) {
      // The native dialog, filtered to the one extension this app reads.
      const chosen = await openDialog({
        multiple: false,
        directory: false,
        filters: [{ name: "Pounce crawl", extensions: ["pounce"] }],
      });
      if (typeof chosen !== "string") return;
      target = chosen;
    }
    setBusy(true);
    setError(null);
    try {
      const opened = await openCrawl(target);
      setHandle(opened);
      setRecent(remember(opened.path, opened.pages));
      setSetup(false);
      setEpoch((e) => e + 1);
    } catch (e) {
      setError(describe(e));
      // A recent whose file has gone is not a recent. Dropping it here rather
      // than leaving it to be clicked again is the difference between an error
      // and an error you have to keep dismissing.
      if (asApiError(e)?.kind === "missing") setRecent(forget(target));
    } finally {
      setBusy(false);
    }
  }

  async function close() {
    await closeCrawl().catch(() => {});
    setHandle(null);
    setLive(null);
  }

  /// Starts a crawl and switches to the results screen immediately.
  ///
  /// The results screen opens the file as soon as the first tick reports
  /// written work — that is T4.21, and it only works because the run is owned
  /// here rather than by the form that started it.
  async function start(settings: CrawlSettings) {
    setBusy(true);
    setError(null);
    let opened = false;
    try {
      const finished = await startCrawl(settings, (p) => {
        setLive({ path: settings.output, progress: p });
        if (!opened && p.written > 0) {
          opened = true;
          void open(settings.output);
        }
      });
      remember(finished.path, finished.pages);
      setLive(null);
      // Reopen: the finished file has its query indices and its site-rule
      // findings, neither of which existed in the snapshot the pane has been
      // reading.
      await open(finished.path);
    } catch (e) {
      setError(describe(e));
      setLive(null);
      setSetup(!opened);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex h-full flex-col bg-canvas text-fg">
      {/* With `titleBarStyle: Overlay` this header IS the window chrome: the
          traffic lights float over its left edge (hence the macOS-only left
          padding) and `data-tauri-drag-region` makes the empty parts of it
          drag the window, the way a real titlebar does. */}
      <header
        data-tauri-drag-region
        className={`flex items-center gap-3 border-b border-border bg-surface px-4 py-2.5 ${
          navigator.userAgent.includes("Mac") ? "pl-24" : ""
        }`}
      >
        <span className="text-lg font-semibold">Pounce</span>
        {handle && (
          <span
            className="min-w-0 truncate text-sm text-fg-muted"
            title={handle.path}
          >
            {basename(handle.path)}
            <span className="ml-2 text-fg-faint">
              {(live ? live.progress.written : handle.pages).toLocaleString()}{" "}
              pages
            </span>
          </span>
        )}
        <div className="flex flex-1 items-center justify-end gap-2">
          {screen !== "setup" && (
            <button
              onClick={() => {
                setError(null);
                setSetup(true);
              }}
              disabled={live !== null}
              title={live ? "A crawl is already running" : undefined}
              className="btn btn-primary"
            >
              New crawl
            </button>
          )}
          {screen !== "setup" && (
            <button onClick={() => void open()} disabled={busy} className="btn">
              {busy ? "Opening…" : "Open…"}
            </button>
          )}
          {handle && live === null && (
            <button onClick={() => void close()} className="btn">
              Close
            </button>
          )}
          <About handle={handle} />
          <div className="ml-1 flex rounded-md border border-border bg-raised p-0.5">
            {THEMES.map((t) => (
              <button
                key={t}
                onClick={() => {
                  setChoiceState(t);
                  setResolved(setChoice(t));
                }}
                aria-pressed={choice === t}
                title={
                  t === "system" ? `Follow the system — now ${resolved}` : undefined
                }
                // `aria-pressed` already carries the selected look and outranks
                // `.btn-primary` on specificity, so the pressed state is stated
                // once, in CSS, rather than twice.
                className={`btn capitalize ${
                  choice === t ? "" : "border-transparent bg-transparent"
                }`}
              >
                {t}
              </button>
            ))}
          </div>
        </div>
      </header>

      {live && <RunStrip progress={live.progress} />}

      {error && screen === "results" && (
        <p className="border-b border-border bg-critical-dim px-4 py-2 text-sm text-critical">
          {error.message}
        </p>
      )}

      {screen === "setup" && (
        <NewCrawl
          busy={busy}
          error={error}
          // The fix is applied to the form's own settings and started again, so
          // "Save as site-2.pounce instead" is one click rather than a retyped
          // path.
          onFix={(fix) => {
            setError(null);
            setPending(fix);
          }}
          pending={pending}
          onStart={(settings) => void start(settings)}
          onCancel={() => setSetup(false)}
        />
      )}

      {screen === "welcome" && (
        <Welcome
          recent={recent}
          error={error?.message ?? null}
          onNew={() => {
            setError(null);
            setSetup(true);
          }}
          onOpen={(path) => void open(path)}
          onForget={(path) => setRecent(forget(path))}
        />
      )}

      {screen === "results" && handle && (
        <Results key={epoch} handle={handle} live={live} rules={rules} />
      )}
    </div>
  );
}
