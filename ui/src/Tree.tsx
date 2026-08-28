import { useEffect, useState } from "react";
import { siteStructure, type StructureNode } from "./engine";
import { useDelayed } from "./useDelayed";

/// The crawl as its folders, one level at a time.
///
/// The other half of the same question the grid answers. A list of 4,000 URLs
/// says nothing about a site's shape; `/baseball/new-jersey/` holding 3,900 of
/// them says most of it, and it is the view a client recognises without being
/// taught the table.
///
/// Each folder is fetched when it is opened, never before: the engine groups
/// by the next path segment and returns counts, so a million-page crawl is the
/// same amount of data on screen as a hundred-page one.
function Row({
  node,
  depth,
  open,
  onToggle,
  onOpen,
  selected,
}: {
  node: StructureNode;
  depth: number;
  open: boolean;
  onToggle: () => void;
  onOpen: (id: number) => void;
  selected: boolean;
}) {
  const clickable = node.folder || node.pageId !== null;
  return (
    <button
      onClick={() => {
        // A folder expands; a page opens. A folder that also has a page at its
        // own address does both jobs from one row, so it expands here and
        // offers its page as the first thing inside.
        if (node.folder) onToggle();
        else if (node.pageId !== null) onOpen(node.pageId);
      }}
      disabled={!clickable}
      aria-expanded={node.folder ? open : undefined}
      // `h-[38px]`, matching the grid's `ROW_HEIGHT`. List and Tree are two
      // readings of one crawl and switching between them should not change the
      // density of the page.
      className={`grid h-[38px] w-full grid-cols-[1fr_5rem_7rem] items-center gap-2 border-b border-border/60 px-3 text-left ${
        selected ? "bg-accent-dim" : "hover:bg-raised"
      }`}
    >
      <span
        className="flex min-w-0 items-center gap-1.5"
        style={{ paddingLeft: `${depth * 1.25}rem` }}
      >
        <span
          aria-hidden
          className={`w-3 shrink-0 text-xs text-fg-faint ${
            node.folder ? "" : "opacity-0"
          }`}
        >
          {open ? "▾" : "▸"}
        </span>
        <span
          className={`tabular truncate ${
            node.folder ? "text-fg" : "text-accent-fg"
          }`}
        >
          {/* The home page of a folder has nothing after the prefix. Naming it
              after the folder it is in is what a person expects to read. */}
          {node.segment === "" ? "(this folder’s own page)" : node.segment}
        </span>
      </span>
      <span className="nums text-right text-sm text-fg-muted">
        {node.pages.toLocaleString()}
      </span>
      <span
        className={`nums text-right text-sm ${
          node.withIssues > 0 ? "text-warning" : "text-fg-faint"
        }`}
      >
        {node.withIssues.toLocaleString()}
      </span>
    </button>
  );
}

export function Tree({
  enabled,
  selectedId,
  refreshKey,
  onOpen,
}: {
  /// `false` before a crawl is open. The tree still draws its header and says
  /// it has nothing, the way every other region of this app does at rest.
  enabled: boolean;
  selectedId: number | null;
  /// Bumped once a second while a crawl writes. Every level already open is
  /// re-read on a change: a tree of counts that stops counting while the crawl
  /// runs is worse than no tree, because it looks finished.
  refreshKey: number;
  onOpen: (id: number) => void;
}) {
  /// Children by prefix, filled as folders are opened. `undefined` means not
  /// asked yet, which is a different thing from a folder with no children.
  const [children, setChildren] = useState<Map<string, StructureNode[]>>(
    new Map(),
  );
  const [more, setMore] = useState<Map<string, number>>(new Map());
  const [open, setOpen] = useState<Set<string>>(new Set());
  const [root, setRoot] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const loading = useDelayed(enabled && root === null && error === null);

  async function load(prefix: string | null) {
    const level = await siteStructure(prefix);
    setChildren((c) => new Map(c).set(level.prefix, level.nodes));
    setMore((m) => new Map(m).set(level.prefix, level.notListed));
    return level.prefix;
  }

  useEffect(() => {
    if (!enabled) {
      setChildren(new Map());
      setOpen(new Set());
      setRoot(null);
      return;
    }
    let current = true;
    setError(null);
    load(null)
      .then((prefix) => current && setRoot(prefix))
      .catch((e) => current && setError(String((e as Error)?.message ?? e)));
    return () => {
      current = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled]);

  useEffect(() => {
    if (!enabled || root === null) return;
    for (const prefix of children.keys()) void load(prefix).catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refreshKey]);

  function toggle(prefix: string) {
    setOpen((o) => {
      const next = new Set(o);
      if (next.has(prefix)) next.delete(prefix);
      else next.add(prefix);
      return next;
    });
    if (!children.has(prefix)) load(prefix).catch(() => {});
  }

  /// Depth-first, and only through folders the reader has opened. The
  /// recursion is over what is on screen, not over the crawl.
  function rows(prefix: string, depth: number): React.ReactNode[] {
    const nodes = children.get(prefix);
    if (!nodes) return [];
    const out: React.ReactNode[] = [];
    for (const node of nodes) {
      const isOpen = open.has(node.prefix);
      out.push(
        <Row
          key={node.prefix}
          node={node}
          depth={depth}
          open={isOpen}
          onToggle={() => toggle(node.prefix)}
          onOpen={onOpen}
          selected={node.pageId !== null && node.pageId === selectedId}
        />,
      );
      if (isOpen) {
        // The folder's own page, when it has one at the address without the
        // slash. It is merged into the folder row upstairs so the two do not
        // read as a duplicate, which leaves this as the only way to open it.
        if (node.pageId !== null) {
          const id = node.pageId;
          out.push(
            <button
              key={`${node.prefix}-self`}
              onClick={() => onOpen(id)}
              className={`grid h-[38px] w-full grid-cols-[1fr_5rem_7rem] items-center gap-2 border-b border-border/60 px-3 text-left ${
                id === selectedId ? "bg-accent-dim" : "hover:bg-raised"
              }`}
            >
              <span
                className="truncate text-sm text-accent-fg"
                style={{ paddingLeft: `${(depth + 1) * 1.25 + 1.25}rem` }}
              >
                This folder’s own page
              </span>
              <span />
              <span />
            </button>,
          );
        }
        out.push(...rows(node.prefix, depth + 1));
      }
    }
    const hidden = more.get(prefix) ?? 0;
    if (hidden > 0) {
      out.push(
        <p
          key={`${prefix}-more`}
          className="border-b border-border/60 px-3 py-1.5 text-sm text-fg-faint"
          style={{ paddingLeft: `${depth * 1.25 + 1.75}rem` }}
        >
          {hidden.toLocaleString()} more not listed
        </p>,
      );
    }
    return out;
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="grid shrink-0 grid-cols-[1fr_5rem_7rem] gap-2 border-b border-border bg-surface px-3 py-1.5 text-xs font-semibold tracking-[0.07em] text-fg-faint uppercase">
        <span>Folder</span>
        <span className="text-right">Pages</span>
        <span className="text-right">With issues</span>
      </div>
      <div className="min-h-0 flex-1 overflow-auto">
        {error && <p className="p-4 text-sm text-critical">{error}</p>}
        {!enabled && (
          <p className="p-4 text-sm text-fg-faint">
            No data. Crawl a site or open a saved crawl to see its shape.
          </p>
        )}
        {loading && <p className="p-4 text-sm text-fg-faint">Reading…</p>}
        {root !== null && (
          <>
            <div className="tabular border-b border-border px-3 py-1.5 text-sm text-fg-muted">
              {root}
            </div>
            {rows(root, 0)}
          </>
        )}
      </div>
    </div>
  );
}
