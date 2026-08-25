import { useEffect, useState } from "react";
import type { BodyKind, Filter } from "./engine";

/// What the filter bar holds. Strings throughout, because these come from
/// `<select>` and `<input>` and an empty string is the one value every control
/// agrees means "not filtering on this".
export type FilterState = {
  urlContains: string;
  /// `"bad"` is 4xx and 5xx together — one range rather than two classes,
  /// because "show me what is broken" is the question people actually ask.
  statusClass: "" | "2" | "3" | "4" | "5" | "bad";
  kind: "" | BodyKind;
  indexable: "" | "yes" | "no";
  maxDepth: "" | "0" | "1" | "2" | "3" | "5";
};

export const NO_FILTERS: FilterState = {
  urlContains: "",
  statusClass: "",
  kind: "",
  indexable: "",
  maxDepth: "",
};

export const isFiltering = (f: FilterState) =>
  Object.values(f).some((v) => v !== "");

/// The engine filters a state compiles to.
///
/// A status *class* is two range filters, not one: the store has `status`, not
/// `status_class`, and `>= 400 AND <= 499` is what a 4xx is. Both are ranges,
/// which is why picking one narrows the sorts on offer — `supported_sorts`
/// says which, rather than this file guessing.
export function toFilters(f: FilterState): Filter[] {
  const out: Filter[] = [];
  if (f.urlContains.trim() !== "")
    out.push({ field: "urlContains", needle: f.urlContains.trim() });
  if (f.statusClass === "bad") {
    out.push({ field: "status", cmp: "ge", value: 400 });
  } else if (f.statusClass !== "") {
    const base = Number(f.statusClass) * 100;
    out.push({ field: "status", cmp: "ge", value: base });
    out.push({ field: "status", cmp: "le", value: base + 99 });
  }
  if (f.kind !== "") out.push({ field: "kind", value: f.kind });
  if (f.indexable !== "")
    out.push({ field: "noindex", value: f.indexable === "no" });
  if (f.maxDepth !== "")
    out.push({ field: "depth", cmp: "le", value: Number(f.maxDepth) });
  return out;
}

function Select<T extends string>({
  label,
  value,
  onChange,
  options,
}: {
  label: string;
  value: T;
  onChange: (v: T) => void;
  options: [T, string][];
}) {
  return (
    <label className="flex items-center gap-1.5">
      <span className="sr-only">{label}</span>
      <select
        value={value}
        onChange={(e) => onChange(e.target.value as T)}
        aria-label={label}
        className={`field text-sm ${value === "" ? "text-fg-muted" : "field-set"}`}
      >
        {options.map(([v, text]) => (
          <option key={v} value={v}>
            {text}
          </option>
        ))}
      </select>
    </label>
  );
}

/// The controls over one crawl's rows.
///
/// Written as words rather than as the columns they compile to: an account
/// manager reads "Broken — 4xx", a specialist reads the 4xx, and both are
/// looking at the same filter.
export function FilterBar({
  value,
  onChange,
}: {
  value: FilterState;
  onChange: (next: FilterState) => void;
}) {
  // The URL box types locally and reports on a pause. Every keystroke is a
  // query against the whole table otherwise, and a substring filter is the one
  // shape no index serves.
  const [needle, setNeedle] = useState(value.urlContains);
  useEffect(() => setNeedle(value.urlContains), [value.urlContains]);
  useEffect(() => {
    if (needle === value.urlContains) return;
    const id = setTimeout(() => onChange({ ...value, urlContains: needle }), 250);
    return () => clearTimeout(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [needle]);

  const set = <K extends keyof FilterState>(key: K, v: FilterState[K]) =>
    onChange({ ...value, [key]: v });

  return (
    <div className="flex flex-wrap items-center gap-1.5">
      <input
        value={needle}
        onChange={(e) => setNeedle(e.target.value)}
        placeholder="Find in URL…"
        aria-label="Find in URL"
        spellCheck={false}
        className={`field nums w-56 placeholder:text-fg-faint ${
          needle === "" ? "" : "field-set"
        }`}
      />
      <Select
        label="Response"
        value={value.statusClass}
        onChange={(v) => set("statusClass", v)}
        options={[
          ["", "Any response"],
          ["bad", "Broken — 4xx and 5xx"],
          ["2", "Worked — 2xx"],
          ["3", "Redirected — 3xx"],
          ["4", "Not found — 4xx"],
          ["5", "Server error — 5xx"],
        ]}
      />
      <Select
        label="Type"
        value={value.kind}
        onChange={(v) => set("kind", v)}
        options={[
          ["", "Any type"],
          ["html", "Pages"],
          ["pdf", "PDFs"],
          ["image", "Images"],
          ["other", "Other files"],
          ["undeclared", "No type declared"],
        ]}
      />
      <Select
        label="Indexing"
        value={value.indexable}
        onChange={(v) => set("indexable", v)}
        options={[
          ["", "Indexable or not"],
          ["yes", "Indexable"],
          ["no", "Noindex"],
        ]}
      />
      <Select
        label="Depth"
        value={value.maxDepth}
        onChange={(v) => set("maxDepth", v)}
        options={[
          ["", "Any depth"],
          ["0", "Home page only"],
          ["1", "1 click from home"],
          ["2", "2 clicks or fewer"],
          ["3", "3 clicks or fewer"],
          ["5", "5 clicks or fewer"],
        ]}
      />
      {isFiltering(value) && (
        <button
          onClick={() => onChange(NO_FILTERS)}
          className="btn"
        >
          Clear
        </button>
      )}
    </div>
  );
}
