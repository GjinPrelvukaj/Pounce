import type { Filter, IssueOverview } from "./engine";
import { severity } from "./severity";

/// What the issue list has selected: nothing, every issue, or one rule.
///
/// `"*"` rather than `null` for "any issue" because `null` already means
/// "no filter" in this UI, and the two produce very different grids.
export type IssueSelection = string | null;

/// The filter a selection compiles to. `hasIssue` with a null rule is the
/// engine's own "any issue at all", so the two cases stay one filter shape.
export function selectionFilters(selection: IssueSelection): Filter[] {
  if (selection === null) return [];
  return [{ field: "hasIssue", rule: selection === "*" ? null : selection }];
}

/// The issue overview, as buttons.
///
/// This is the interaction the product was missing: a count that cannot be
/// opened is a statistic. Every row here filters the grid to the pages it
/// counts, so "3,913 pages are noindexed" becomes those 3,913 pages.
export function IssueList({
  overview,
  selection,
  live,
  onSelect,
}: {
  overview: IssueOverview;
  selection: IssueSelection;
  /// Whether a crawl is still writing this file. An empty overview means two
  /// very different things either side of that, and saying the wrong one is
  /// a clean bill of health the crawl has not earned yet.
  live?: boolean;
  onSelect: (next: IssueSelection) => void;
}) {
  if (overview.byRule.length === 0) {
    return (
      <p className="text-xs text-fg-muted">
        {live
          ? "No issues yet — findings appear as each batch of pages is saved."
          : "No issues found — every rule this build has passed on every page."}
      </p>
    );
  }

  return (
    <div className="flex flex-wrap gap-1.5">
      <Chip
        label="All issues"
        count={overview.urlsWithIssues}
        icon="◆"
        tone="text-accent-fg"
        active={selection === "*"}
        onClick={() => onSelect(selection === "*" ? null : "*")}
        title={`${overview.totalIssues.toLocaleString()} findings across ${overview.urlsWithIssues.toLocaleString()} URLs`}
      />
      {overview.byRule.map((r) => {
        const sev = severity(r.severity);
        const active = selection === r.ruleId;
        return (
          <Chip
            key={`${r.ruleId}-${r.severity}`}
            label={r.ruleId}
            count={r.urls}
            icon={sev.icon}
            tone={sev.tone}
            active={active}
            onClick={() => onSelect(active ? null : r.ruleId)}
            title={`${sev.label} · ${r.issues.toLocaleString()} findings on ${r.urls.toLocaleString()} URLs`}
          />
        );
      })}
    </div>
  );
}

function Chip({
  label,
  count,
  icon,
  tone,
  active,
  onClick,
  title,
}: {
  label: string;
  count: number;
  icon: string;
  tone: string;
  active: boolean;
  onClick: () => void;
  title: string;
}) {
  return (
    <button
      onClick={onClick}
      title={title}
      aria-pressed={active}
      className={`tabular flex items-center gap-1.5 rounded-sm border px-2 py-1 text-xs transition-colors duration-150 ease-state focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent ${
        active
          ? "border-accent-line bg-accent-dim text-fg"
          : "border-border bg-raised text-fg-muted hover:border-border-strong hover:text-fg"
      }`}
    >
      <span aria-hidden className={tone}>
        {icon}
      </span>
      {label}
      <span className={active ? "text-fg" : "text-fg-faint"}>
        {count.toLocaleString()}
      </span>
    </button>
  );
}
