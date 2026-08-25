import { COLUMNS } from "./Grid";

/// Which grid columns this person wants, remembered between sessions.
///
/// In the window's own storage, like the recents list and the theme: a column
/// layout is a property of the person, not of any `.pounce` file, and a crawl
/// handed to a colleague should not arrive carrying someone else's preferences.

const KEY = "pounce.columns";

/// The columns a new install shows. Status, URL and title identify the row;
/// the numbers are what people sort by. Type and indexability are real
/// columns, off by default because the filter bar already answers both
/// questions and the grid is better narrow.
export const DEFAULT_COLUMNS = [
  "row",
  "status",
  "url",
  "title",
  "wordCount",
  "depth",
  "size",
];

const ALL = () => COLUMNS.map((c) => c.key as string);

export function storedColumns(): string[] {
  try {
    const raw = JSON.parse(localStorage.getItem(KEY) ?? "null") as unknown;
    if (!Array.isArray(raw)) return DEFAULT_COLUMNS;
    // Filtered against the build's own column list: a stored key from a
    // version that had a column this one does not would otherwise render an
    // empty track, and dropping it silently is right — the layout is a
    // preference, not data.
    const known = new Set(ALL());
    const kept = raw.filter((k): k is string => typeof k === "string" && known.has(k));
    return kept.length > 0 ? kept : DEFAULT_COLUMNS;
  } catch {
    return DEFAULT_COLUMNS;
  }
}

export function saveColumns(keys: string[]): string[] {
  // Never store an empty layout: a grid with no columns is a grid you cannot
  // get back from without clearing storage.
  const kept = keys.length > 0 ? keys : DEFAULT_COLUMNS;
  try {
    localStorage.setItem(KEY, JSON.stringify(kept));
  } catch {
    // A full or disabled store is not a reason to refuse the change for this
    // session.
  }
  return kept;
}

/// Toggling keeps `COLUMNS` order, so the grid never reshuffles under the
/// hand that ticked a box.
export function toggleColumn(keys: string[], key: string): string[] {
  const next = keys.includes(key)
    ? keys.filter((k) => k !== key)
    : [...keys, key];
  return ALL().filter((k) => next.includes(k));
}
