import { ago, basename, type Recent } from "./recents";

/// The first thing a new user sees, and the thing they come back to between
/// crawls.
///
/// Before this the window opened on a form with no explanation of what the app
/// was for. An empty state is not decoration: it is the only screen that gets
/// to say what this program does.
export function Welcome({
  recent,
  onNew,
  onOpen,
  onForget,
  error,
}: {
  recent: Recent[];
  onNew: () => void;
  /// With a path, open that file; without one, ask for one.
  onOpen: (path?: string) => void;
  onForget: (path: string) => void;
  error: string | null;
}) {
  return (
    <main className="flex min-h-0 flex-1 flex-col items-center justify-center gap-6 p-8">
      <div className="flex max-w-xl flex-col items-center gap-3 text-center">
        <h2 className="text-lg font-semibold">Crawl a site and see what is wrong with it</h2>
        <p className="text-sm text-fg-muted">
          Pounce fetches every page, checks thirty things about each one, and
          keeps the whole crawl in a file you can reopen. Results appear while
          the crawl is still running.
        </p>
      </div>

      <div className="flex gap-2">
        <button onClick={onNew} className="btn btn-primary px-4 py-2">
          New crawl
        </button>
        <button onClick={() => onOpen()} className="btn px-4 py-2">
          Open a saved crawl…
        </button>
      </div>

      {error && <p className="text-sm text-critical">{error}</p>}

      {recent.length > 0 && (
        <div className="flex w-full max-w-xl flex-col gap-1">
          <h3 className="text-xs font-semibold tracking-[0.07em] text-fg-faint uppercase">
            Recent
          </h3>
          <ul className="flex flex-col">
            {recent.map((r) => (
              <li
                key={r.path}
                className="flex items-center gap-2 border-b border-border/60 py-1.5"
              >
                <button
                  onClick={() => onOpen(r.path)}
                  title={r.path}
                  className="focusable nums min-w-0 flex-1 truncate rounded-sm text-left text-sm text-accent-fg hover:underline"
                >
                  {basename(r.path)}
                </button>
                <span className="nums shrink-0 text-xs text-fg-faint">
                  {r.pages.toLocaleString()} pages · {ago(r.openedAt)}
                </span>
                <button
                  onClick={() => onForget(r.path)}
                  aria-label={`Remove ${basename(r.path)} from recent crawls`}
                  className="focusable shrink-0 rounded-sm px-1 text-xs text-fg-faint transition-colors duration-150 ease-state hover:text-critical"
                >
                  ✕
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </main>
  );
}
