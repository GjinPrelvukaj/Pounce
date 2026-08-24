/// The recently opened crawls.
///
/// Kept in the window's own storage rather than in the engine: a recents list
/// is a property of this person's app, not of any `.pounce` file, and a crawl
/// file handed to someone else should not arrive carrying a list of where it
/// has been.

export type Recent = {
  path: string;
  /// Milliseconds since the epoch, so the list can be ordered without asking
  /// the filesystem.
  openedAt: number;
  pages: number;
};

const KEY = "pounce.recents";

/// How many to keep. Long enough to cover a working session, short enough that
/// the list stays scannable — this is a shortcut, not a history.
const LIMIT = 8;

export function recents(): Recent[] {
  try {
    const raw = JSON.parse(localStorage.getItem(KEY) ?? "[]") as unknown;
    if (!Array.isArray(raw)) return [];
    return raw
      .filter(
        (r): r is Recent =>
          typeof r === "object" &&
          r !== null &&
          typeof (r as Recent).path === "string",
      )
      .slice(0, LIMIT);
  } catch {
    // Corrupt storage is not worth a crash on launch; an empty list is a
    // recoverable state and the next open repairs it.
    return [];
  }
}

/// Records a crawl as most recent, replacing any earlier entry for the same
/// file rather than accumulating duplicates of the one file people reopen.
export function remember(path: string, pages: number): Recent[] {
  const next = [
    { path, openedAt: Date.now(), pages },
    ...recents().filter((r) => r.path !== path),
  ].slice(0, LIMIT);
  localStorage.setItem(KEY, JSON.stringify(next));
  return next;
}

export function forget(path: string): Recent[] {
  const next = recents().filter((r) => r.path !== path);
  localStorage.setItem(KEY, JSON.stringify(next));
  return next;
}

/// The last path segment, for a list that has to stay readable at a glance.
/// The full path stays available as a tooltip.
export function basename(path: string): string {
  return path.split("/").pop() || path;
}

/// Rough, and deliberately so: "3m ago" is what this list is for, and a
/// precise timestamp would be noise beside a filename.
export function ago(then: number): string {
  const seconds = Math.max(0, Math.round((Date.now() - then) / 1000));
  if (seconds < 60) return "just now";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.round(hours / 24)}d ago`;
}
