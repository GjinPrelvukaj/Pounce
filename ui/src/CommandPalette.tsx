import { useEffect, useMemo, useRef, useState } from "react";

/// One thing the palette can do.
export type Command = {
  id: string;
  label: string;
  group: string;
  /// Right-aligned metadata: a count, a current value, a shortcut.
  hint?: string;
  run: () => void;
};

/// Subsequence match, the way every command palette does it: "ntx" finds "Not
/// indexable". Scores earlier and more contiguous matches higher so exact
/// prefixes win, which is what people expect when they type two letters and
/// hit Enter without looking.
function score(label: string, query: string): number {
  if (query === "") return 1;
  const l = label.toLowerCase();
  const q = query.toLowerCase();
  if (l.startsWith(q)) return 1000 - label.length;
  let li = 0;
  let hits = 0;
  let streak = 0;
  let best = 0;
  for (const ch of q) {
    const found = l.indexOf(ch, li);
    if (found < 0) return -1;
    streak = found === li ? streak + 1 : 1;
    best = Math.max(best, streak);
    hits += 1;
    li = found + 1;
  }
  return hits * 10 + best * 5 - li;
}

/// Everything the app can do, one keystroke away.
///
/// A specialist crawling sites daily should never need the mouse, and before
/// this the keyboard reached the grid and nothing else: not the nine view tabs,
/// not the filters, not the findings. Those are the things you switch between
/// twenty times in a session.
///
/// A native `<dialog>` again, for the backdrop, the focus trap and Escape.
export function CommandPalette({
  commands,
  open,
  onClose,
}: {
  commands: Command[];
  open: boolean;
  onClose: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const [query, setQuery] = useState("");
  const [cursor, setCursor] = useState(0);

  const matches = useMemo(() => {
    const scored = commands
      .map((c) => ({ c, s: score(c.label, query) }))
      .filter((m) => m.s >= 0);
    scored.sort((a, b) => b.s - a.s);
    return scored.slice(0, 40).map((m) => m.c);
  }, [commands, query]);

  useEffect(() => {
    const el = dialog.current;
    if (!el) return;
    if (open && !el.open) {
      setQuery("");
      setCursor(0);
      el.showModal();
    } else if (!open && el.open) {
      el.close();
    }
  }, [open]);

  // Keep the highlighted row on screen when arrowing past the fold.
  useEffect(() => {
    list.current
      ?.querySelector('[data-active="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }, [cursor, matches]);

  const runAt = (i: number) => {
    const command = matches[i];
    if (!command) return;
    onClose();
    command.run();
  };

  let lastGroup = "";

  return (
    <dialog
      ref={dialog}
      onCancel={(e) => {
        e.preventDefault();
        onClose();
      }}
      onClick={(e) => e.target === dialog.current && onClose()}
      className="raised-panel mx-auto mt-[12vh] w-[34rem] max-w-[92vw] rounded-lg border border-border bg-surface p-0 text-fg"
    >
      <input
        autoFocus
        value={query}
        onChange={(e) => {
          setQuery(e.target.value);
          setCursor(0);
        }}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown") {
            e.preventDefault();
            setCursor((c) => Math.min(c + 1, matches.length - 1));
          } else if (e.key === "ArrowUp") {
            e.preventDefault();
            setCursor((c) => Math.max(c - 1, 0));
          } else if (e.key === "Enter") {
            e.preventDefault();
            runAt(cursor);
          }
        }}
        placeholder="Search views, findings and actions…"
        aria-label="Command palette"
        // `outline-none` with a replacement, not without one: the bottom border
        // takes the accent on focus, the way every `.field` in the app does.
        className="w-full border-0 border-b border-border bg-transparent px-4 py-3 text-md text-fg outline-none focus:border-accent-line placeholder:text-fg-faint"
      />

      <div ref={list} className="max-h-[46vh] overflow-auto p-2">
        {matches.length === 0 && (
          <p className="px-3 py-6 text-center text-sm text-fg-muted">
            Nothing matches “{query}”.
          </p>
        )}
        {matches.map((command, i) => {
          const heading = command.group !== lastGroup ? command.group : null;
          lastGroup = command.group;
          return (
            <div key={command.id}>
              {heading && (
                <h3 className="px-3 pt-3 pb-1 text-xs font-semibold tracking-[0.07em] text-fg-faint uppercase">
                  {heading}
                </h3>
              )}
              <button
                data-active={i === cursor}
                onMouseMove={() => setCursor(i)}
                onClick={() => runAt(i)}
                className={`flex w-full items-center gap-2 rounded-sm px-3 py-2 text-left text-sm ${
                  i === cursor ? "bg-accent-dim text-fg" : "text-fg-muted"
                }`}
              >
                <span className="min-w-0 flex-1 truncate">{command.label}</span>
                {command.hint && (
                  <span className="nums shrink-0 text-xs text-fg-faint">
                    {command.hint}
                  </span>
                )}
              </button>
            </div>
          );
        })}
      </div>
    </dialog>
  );
}
