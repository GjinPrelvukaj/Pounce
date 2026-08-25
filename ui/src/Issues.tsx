import type { Filter, IssueOverview, RuleInfo } from "./engine";
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

/// The issue overview, as findings.
///
/// Two jobs at once, and they used to fight: this is the summary of what is
/// wrong with the site *and* the primary navigation into it. A count that
/// cannot be opened is a statistic, and `indexability.canonical-elsewhere` is
/// a database key — so each row is a button, and each button reads as the
/// sentence the registry has carried since M2.
export function IssueList({
  overview,
  rules,
  selection,
  live,
  onSelect,
}: {
  overview: IssueOverview;
  /// Rule id → its sentence. Empty until the engine answers, and a file
  /// written by a build with rules this one lacks will have gaps in it — so
  /// every read falls back to the id rather than to a blank row.
  rules: Map<string, RuleInfo>;
  selection: IssueSelection;
  /// Whether a crawl is still writing this file. An empty overview means two
  /// very different things either side of that, and saying the wrong one is a
  /// clean bill of health the crawl has not earned yet.
  live?: boolean;
  onSelect: (next: IssueSelection) => void;
}) {
  if (overview.byRule.length === 0) {
    return (
      <p className="text-md text-fg-muted">
        {live
          ? "No issues yet — findings appear as each batch of pages is saved."
          : "No issues found — every rule this build has passed on every page."}
      </p>
    );
  }

  const chosen = selection && selection !== "*" ? rules.get(selection) : null;

  return (
    <div className="flex flex-col gap-2">
      <div className="grid gap-1 [grid-template-columns:repeat(auto-fill,minmax(24rem,1fr))]">
        <Finding
          text="Every page with something to fix"
          meta={`${overview.totalIssues.toLocaleString()} findings in total`}
          count={overview.urlsWithIssues}
          icon="◆"
          tone="text-accent-fg"
          active={selection === "*"}
          onClick={() => onSelect(selection === "*" ? null : "*")}
        />
        {overview.byRule.map((r) => {
          const sev = severity(r.severity);
          const rule = rules.get(r.ruleId);
          const active = selection === r.ruleId;
          return (
            <Finding
              key={`${r.ruleId}-${r.severity}`}
              text={rule?.description ?? r.ruleId}
              meta={`${sev.label} · ${r.ruleId} · ${r.issues.toLocaleString()} findings`}
              count={r.urls}
              icon={sev.icon}
              tone={sev.tone}
              active={active}
              onClick={() => onSelect(active ? null : r.ruleId)}
            />
          );
        })}
      </div>

      {/* The other half of a finding. A rule that says what is wrong and not
          what to do about it is a complaint. */}
      {chosen && (
        <p className="text-sm text-fg-muted">
          <span className="text-fg-faint">Fix:</span> {chosen.remediation}
        </p>
      )}
    </div>
  );
}

/// One finding: the sentence, what it counts, and — quietly — the key it is
/// filtered by. The id stays reachable because `--fail-on` and exported
/// reports use it, but it is metadata now, not the headline.
function Finding({
  text,
  meta,
  count,
  icon,
  tone,
  active,
  onClick,
}: {
  text: string;
  meta: string;
  count: number;
  icon: string;
  tone: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      title={meta}
      aria-pressed={active}
      className={`flex items-center gap-2 rounded-sm border px-2 py-1 text-left text-md transition-colors duration-150 ease-state focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent ${
        active
          ? "border-accent-line bg-accent-dim text-fg"
          : "border-transparent text-fg-muted hover:border-border hover:bg-raised hover:text-fg"
      }`}
    >
      <span aria-hidden className={`${tone} shrink-0`}>
        {icon}
      </span>
      <span className="min-w-0 flex-1 truncate">{text}</span>
      <span
        className={`tabular shrink-0 text-sm ${active ? "text-fg" : "text-fg-faint"}`}
      >
        {count.toLocaleString()}
      </span>
    </button>
  );
}
