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
    <div className="flex min-w-0 flex-col gap-2">
      <div className="flex min-w-0 flex-col gap-0.5">
        <Finding
          text="Every page with something to fix"
          meta={`${overview.totalIssues.toLocaleString()} findings in total`}
          count={overview.urlsWithIssues}
          icon="◆"
          tone="text-accent-fg"
          active={selection === "*"}
          onClick={() => onSelect(selection === "*" ? null : "*")}
        />
        {grouped(overview).map(([name, counts]) => (
          <div key={name} className="flex min-w-0 flex-col gap-0.5">
            {/* The heading carries no number on purpose. The rows below count
                URLs, `bySeverity` counts findings, and two different units
                stacked on top of each other is the confusion this rail exists
                to remove. */}
            <h3 className="mt-2 flex items-center gap-1.5 text-sm text-fg-faint">
              <span aria-hidden className={severity(name).tone}>
                {severity(name).icon}
              </span>
              {severity(name).label}
            </h3>
            {counts.map((r) => {
              const sev = severity(r.severity);
              const rule = rules.get(r.ruleId);
              const active = selection === r.ruleId;
              return (
                <Finding
                  key={`${r.ruleId}-${r.severity}`}
                  text={rule?.description ?? r.ruleId}
                  meta={`${sev.label} · ${r.ruleId} · ${r.issues.toLocaleString()} findings on ${r.urls.toLocaleString()} URLs`}
                  count={r.urls}
                  // No icon per row: the heading above states the severity in
                  // an icon *and* a word, and repeating the same triangle down
                  // seven rows is noise rather than information. The rule that
                  // a state is never carried by colour alone is satisfied by
                  // the group, and these rows carry no colour to begin with.
                  tone={sev.tone}
                  active={active}
                  onClick={() => onSelect(active ? null : r.ruleId)}
                />
              );
            })}
          </div>
        ))}
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
/// The findings, grouped by severity and worst first.
///
/// The engine returns them ordered by count, which puts a 4,000-page notice
/// above a two-page critical — a fair ordering of numbers and a misleading
/// ordering of *problems*. Within a severity, count order is right: that is
/// the one place a bigger number does mean a bigger job.
function grouped(overview: IssueOverview): [string, IssueOverview["byRule"]][] {
  const groups = new Map<string, IssueOverview["byRule"]>();
  for (const row of overview.byRule) {
    const list = groups.get(row.severity);
    if (list) list.push(row);
    else groups.set(row.severity, [row]);
  }
  return [...groups.entries()].sort(
    ([a], [b]) => severity(a).rank - severity(b).rank,
  );
}

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
  icon?: string;
  tone: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      title={meta}
      aria-pressed={active}
      // `min-w-0` twice over: a flex item's `min-width` defaults to its content,
      // so a long sentence pushes the button past the rail and the count off
      // the end of it. `w-full` alone does not stop that.
      className={`btn w-full min-w-0 justify-start border-transparent bg-transparent text-left text-md ${
        active ? "" : "hover:border-border hover:bg-raised"
      }`}
    >
      {icon ? (
        <span aria-hidden className={`${tone} shrink-0`}>
          {icon}
        </span>
      ) : (
        <span aria-hidden className="w-2 shrink-0" />
      )}
      <span className="min-w-0 flex-1 truncate">{text}</span>
      <span
        className={`tabular shrink-0 text-sm ${active ? "text-fg" : "text-fg-faint"}`}
      >
        {count.toLocaleString()}
      </span>
    </button>
  );
}
