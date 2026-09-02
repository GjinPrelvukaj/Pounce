import { useState } from "react";
import type { CrawlOverview, IssueOverview, RuleInfo } from "./engine";
import type { FilterState } from "./Filters";
import { NO_FILTERS } from "./Filters";
import { severity } from "./severity";
import type { IssueSelection } from "./Issues";

/// One line of the panel: a label, a count, and the thing clicking it does.
export type Line = {
  label: string;
  /// Undefined for a row that is a statement rather than a count —
  /// "robots.txt answered 200" is not zero of anything, and rendering it as
  /// "0" is the panel inventing a measurement.
  count?: number;
  share?: number;
  /// Set the filter bar to this.
  filters?: FilterState;
  /// Or select this rule, on top of whatever the view already filters to.
  rule?: string;
  severity?: string;
  indent?: boolean;
};

const KIND_LABEL: Record<string, string> = {
  html: "Pages",
  pdf: "PDFs",
  image: "Images",
  other: "Other files",
  undeclared: "No type declared",
};

const CLASS_LABEL = [
  "Informational (1xx)",
  "Worked (2xx)",
  "Redirected (3xx)",
  "Not found (4xx)",
  "Server error (5xx)",
];

/// The crawl composition rows, for the views that are about the whole crawl
/// rather than about one aspect of it.
export function summaryLines(o: CrawlOverview): { title: string; rows: Line[] }[] {
  const share = (n: number) => (o.crawled > 0 ? n / o.crawled : 0);
  return [
    {
      title: "Summary",
      rows: [
        {
          label: "Pages crawled",
          count: o.crawled,
          share: 1,
          filters: NO_FILTERS,
        },
        // Neither of these is in `pages`, so neither can filter the grid: a URL
        // still queued has no row, and one that never answered has no row
        // either. A plain number beats a button that shows nothing.
        { label: "Still to fetch", count: o.queued },
        { label: "Never answered", count: o.failed },
      ],
    },
    {
      title: "What was found",
      rows: o.byKind.map(([kind, count]) => ({
        label: KIND_LABEL[kind] ?? kind,
        count,
        share: share(count),
        filters: { ...NO_FILTERS, kind: kind as FilterState["kind"] },
        indent: true,
      })),
    },
    {
      title: "How it answered",
      rows: o.byClass
        .map((count, i) => ({
          label: CLASS_LABEL[i]!,
          count,
          share: share(count),
          filters: {
            ...NO_FILTERS,
            statusClass: String(i + 1) as FilterState["statusClass"],
          },
          indent: true,
        }))
        // A healthy crawl is all 2xx, and four permanent zeroes make a panel
        // you stop reading. Before any crawl every class is zero, though, and
        // hiding them all would drop the section entirely: show the three
        // anyone recognises, so the panel reads as a shape at rest.
        .filter(
          (row, i) => row.count > 0 || (o.crawled === 0 && i >= 1 && i <= 3),
        ),
    },
    // Only when there are any. A permanent "0 pages need JavaScript" row is a
    // reassurance nobody asked for, on every crawl of every static site.
    ...(o.jsShell > 0
      ? [
          {
            title: "Before you read the rest",
            rows: [
              {
                label:
                  "Pages that arrived with almost no text — this site probably builds its pages with JavaScript",
                count: o.jsShell,
                share: share(o.jsShell),
              },
            ],
          },
        ]
      : []),
    {
      title: "Indexing",
      rows: [
        {
          label: "Indexable",
          count: o.indexable,
          share: share(o.indexable),
          filters: { ...NO_FILTERS, indexable: "yes" },
          indent: true,
        },
        {
          label: "Not indexable",
          count: o.noindex,
          share: share(o.noindex),
          filters: { ...NO_FILTERS, indexable: "no" },
          indent: true,
        },
      ],
    },
  ];
}

/// Every rule in `batches`, whether or not it fired.
///
/// **Showing the zeroes is the point.** Screaming Frog's panel lists Missing 0,
/// Duplicate 0, Over 60 Characters 2 — and the zeroes are what tell you the
/// check ran and found nothing, which is a different statement from the rule
/// being absent. A list that only shows what fired cannot say "your titles are
/// fine"; it can only fail to mention them.
export function ruleLines(
  batches: string[],
  rules: Map<string, RuleInfo>,
  issues: IssueOverview | null,
  total: number,
): Line[] {
  const counts = new Map<string, number>();
  const onPages = new Map<string, boolean>();
  for (const row of issues?.byRule ?? []) {
    counts.set(row.ruleId, row.urls);
    onPages.set(row.ruleId, row.pageUrls === row.urls);
  }
  const shareOf = (id: string) =>
    total > 0 && (onPages.get(id) ?? true)
      ? (counts.get(id) ?? 0) / total
      : undefined;

  return [...rules.values()]
    // A batch name lists every rule in it; a full rule id lists just that
    // one. The Duplicates view is three rules from three different batches,
    // and it would otherwise need its own panel rather than a list.
    .filter((r) => batches.includes(r.id) || batches.includes(r.id.split(".")[0]!))
    .map((r) => ({
      label: r.description,
      count: counts.get(r.id) ?? 0,
      // Undefined rather than a wrong number when the rule counts things
      // that are not pages: the row still shows how many, without claiming a
      // proportion of a total they were never part of.
      share: shareOf(r.id),
      rule: r.id,
      severity: r.severity,
      indent: true,
    }))
    // Worst first, then the ones that fired, then the rest alphabetically —
    // so the panel opens on what needs doing without the zeroes moving around
    // between crawls.
    .sort(
      (a, b) =>
        (b.count > 0 ? 1 : 0) - (a.count > 0 ? 1 : 0) ||
        severity(a.severity!).rank - severity(b.severity!).rank ||
        b.count - a.count ||
        a.label.localeCompare(b.label),
    );
}

/// What this build does not check.
///
/// **The panel's own design makes this necessary.** A rule that found nothing
/// is listed at zero on purpose, because "the check ran and found nothing" is
/// a different statement from "there is no such check" — and that convention
/// is exactly what makes an *absent* check dangerous. A reader who sees green
/// zeroes down the panel concludes their hreflang is fine. We never looked.
///
/// So the absences are listed too, in the same panel, plainly. This is
/// cheaper than thirty more rules and more honest than either the rules or the
/// silence would be alone. Each line says what a reader should do instead.
const NOT_CHECKED: { label: string; why: string }[] = [
  {
    label: "hreflang",
    why: "Language and country tags for international sites. Not checked in this version.",
  },
  {
    label: "Structured data",
    why: "Schema.org markup for rich results. Not checked in this version.",
  },
  {
    label: "Pagination",
    why: "rel=next and rel=prev across paged listings. Not checked in this version.",
  },
  {
    label: "JavaScript rendering",
    why: "Pages are read as the server sends them. A site that builds its content with JavaScript will look emptier here than it is.",
  },
  {
    label: "Page speed",
    why: "Response time is measured; Core Web Vitals and rendering performance are not.",
  },
];

export function NotChecked() {
  return (
    <section className="flex min-w-0 flex-col gap-2">
      <h3 className="px-2 text-xs font-semibold tracking-[0.07em] text-fg-faint uppercase">
        Not checked in this version
      </h3>
      <p className="px-2 text-xs text-fg-muted">
        Everything above ran on every page. These did not run at all, so this
        crawl says nothing about them either way.
      </p>
      <ul className="flex flex-col">
        {NOT_CHECKED.map((row) => (
          <li
            key={row.label}
            title={row.why}
            className="flex items-baseline gap-2 px-2 py-1 text-sm text-fg-faint"
          >
            <span aria-hidden className="shrink-0">
              —
            </span>
            <span className="min-w-0 flex-1">{row.label}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}

/// Every finding that occurred, worst first, in the vocabulary a client report
/// uses.
///
/// This is Screaming Frog's Issues tab, and it is the single most
/// client-legible surface in that application: a prioritised worklist rather
/// than a set of filters. It is global on purpose, unlike the Overview panel
/// beside it, which enumerates only the current tab's checks. A rule that
/// found nothing is absent here — "what to fix" is a list of work, and a check
/// that passed is not work.
function IssuesList({
  issues,
  rules,
  total,
  selection,
  onSelectRule,
}: {
  issues: IssueOverview | null;
  rules: Map<string, RuleInfo>;
  total: number;
  selection: IssueSelection;
  onSelectRule: (next: IssueSelection) => void;
}) {
  const rows = (issues?.byRule ?? [])
    .map((r) => {
      const sev = severity(r.severity);
      return { ...r, sev, rule: rules.get(r.ruleId) };
    })
    .sort((a, b) => a.sev.rank - b.sev.rank || b.urls - a.urls);

  if (rows.length === 0) {
    return (
      <p className="px-2 py-6 text-center text-sm text-fg-muted">
        Nothing to fix. Every check this build has passed on every page.
      </p>
    );
  }

  const counts = new Map<string, number>();
  for (const row of rows)
    counts.set(row.sev.type, (counts.get(row.sev.type) ?? 0) + 1);

  return (
    <div className="flex flex-col gap-2">
      {/* The tally Screaming Frog puts above its list: how much of each kind,
          before any of the detail. */}
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 px-2 pb-1">
        {/* Plurals are spelled out rather than built with a trailing "s":
            "Opportunitys" shipped for exactly as long as it took to look at
            the panel. */}
        {(
          [
            ["Issue", "Issues", "text-critical"],
            ["Warning", "Warnings", "text-warning"],
            ["Opportunity", "Opportunities", "text-notice"],
          ] as const
        ).map(([type, plural, tone]) => {
          const n = counts.get(type) ?? 0;
          return (
            <span key={type} className="text-xs text-fg-faint">
              <span className={n > 0 ? tone : ""}>{n === 1 ? type : plural}</span>{" "}
              <span className="nums text-fg">{n}</span>
            </span>
          );
        })}
        <span className="nums ml-auto text-xs text-fg-faint">
          {rows.length} in total
        </span>
      </div>

      <div className="flex min-w-0 flex-col gap-1">
        {rows.map((row) => {
          const active = selection === row.ruleId;
          // No share when the rule's subjects are not pages. Twenty-four
          // oversized images on an eighteen-page site is not "133.3% of the
          // crawl", it is twenty-four images — and the bar behind the row
          // would have been full and then some.
          const share =
            total > 0 && row.pageUrls === row.urls ? row.urls / total : 0;
          return (
            <button
              key={`${row.ruleId}-${row.severity}`}
              onClick={() => onSelectRule(active ? null : row.ruleId)}
              aria-pressed={active}
              title={`${row.sev.type} · ${row.sev.priority} priority · ${row.ruleId}\n\n${row.rule?.remediation ?? ""}`}
              className={`btn relative w-full min-w-0 flex-col items-stretch gap-1 overflow-hidden border-transparent bg-transparent px-2 py-2 text-left shadow-none ${
                active ? "" : "hover:border-border hover:bg-raised"
              }`}
            >
              <span
                aria-hidden
                className={`pointer-events-none absolute inset-y-0.5 left-0 rounded-r ${row.sev.dim}`}
                style={{ width: `${Math.min(share, 1) * 100}%` }}
              />
              <span className="flex min-w-0 items-center gap-2">
                <span aria-hidden className={`shrink-0 ${row.sev.tone}`}>
                  {row.sev.icon}
                </span>
                {/* Wrapped, not truncated. These are sentences from the
                    rule registry — the whole point of writing findings as
                    sentences is lost at "An image the page references is
                    large …", which names no image and no threshold. Two lines
                    is the cap; past that the title attribute has the rest. */}
                <span className="line-clamp-2 min-w-0 flex-1 text-sm text-fg">
                  {row.rule?.description ?? row.ruleId}
                </span>
                <span className="nums shrink-0 text-sm text-fg">
                  {row.urls.toLocaleString()}
                </span>
              </span>
              <span className="flex items-center gap-2 pl-6 text-xs text-fg-faint">
                <span className={row.sev.tone}>{row.sev.type}</span>
                <span>{row.sev.priority} priority</span>
                <span className="nums ml-auto">
                  {share > 0 ? `${(share * 100).toFixed(1)}%` : ""}
                </span>
              </span>
            </button>
          );
        })}
      </div>
    </div>
  );
}

/// The right-hand panel: the filters for the view you are on.
///
/// This is the load-bearing idea taken from Screaming Frog, and the one the
/// first attempt missed. Its right panel is not a crawl summary that stays put
/// — it is **the filter list for the current tab**. On Page Titles it offers
/// Missing, Duplicate, Over 60 Characters; on Images it offers Over 100 kB and
/// Missing Alt Text. That is why the app is usable at this density: the tab
/// narrows the question and the panel enumerates every answer.
///
/// Pounce's rules already *are* that list — `title.*` is the Page Titles panel,
/// `media.*` is the Images panel — so this needed no new engine work, only the
/// realisation that the rule registry was the missing panel all along.
export function Overview({
  title,
  groups,
  issues,
  rules,
  total,
  selection,
  activeFilters,
  onSelectRule,
  onFilter,
}: {
  title: string;
  groups: { title: string; rows: Line[] }[];
  issues: IssueOverview | null;
  rules: Map<string, RuleInfo>;
  total: number;
  selection: IssueSelection;
  /// The filter bar's current state, so a line already applied reads as applied.
  activeFilters: string;
  onSelectRule: (next: IssueSelection) => void;
  onFilter: (filters: FilterState) => void;
}) {
  const [tab, setTab] = useState<"overview" | "issues">("overview");

  return (
    <aside className="flex min-w-0 flex-1 flex-col border-l border-border bg-surface">
      <div className="flex shrink-0 items-center gap-1 border-b border-border px-2 pt-2">
        <button
          onClick={() => setTab("overview")}
          aria-pressed={tab === "overview"}
          className="tab"
        >
          Overview
        </button>
        <button
          onClick={() => setTab("issues")}
          aria-pressed={tab === "issues"}
          className="tab"
        >
          Issues
          {issues && issues.byRule.length > 0 && (
            <span className="nums text-xs text-fg-faint">
              {issues.byRule.length}
            </span>
          )}
        </button>
      </div>

      <div className="flex shrink-0 items-baseline justify-between gap-2 border-b border-border px-4 py-2">
        <h2 className="text-sm font-medium text-fg">
          {tab === "overview" ? title : "What to fix, worst first"}
        </h2>
        <span className="nums text-xs text-fg-faint">URLs · % of total</span>
      </div>

      <div className="min-h-0 flex-1 overflow-auto p-3">
        {tab === "issues" ? (
          <IssuesList
            issues={issues}
            rules={rules}
            total={total}
            selection={selection}
            onSelectRule={onSelectRule}
          />
        ) : (
        <div className="flex flex-col gap-3">
          {groups
            .filter((group) => group.rows.length > 0)
            .map((group) => (
              <section key={group.title} className="flex flex-col gap-1">
                <h3 className="px-2 pt-1 text-xs font-semibold tracking-[0.07em] text-fg-faint uppercase">
                  {group.title}
                </h3>
                {group.rows.map((row) => (
                  <Row
                    key={row.rule ?? row.label}
                    row={row}
                    active={
                      row.rule !== undefined
                        ? selection === row.rule
                        : row.filters !== undefined &&
                          JSON.stringify(row.filters) === activeFilters
                    }
                    onClick={
                      row.rule !== undefined
                        ? () =>
                            onSelectRule(selection === row.rule ? null : row.rule!)
                        : row.filters !== undefined
                          ? () => onFilter(row.filters!)
                          : undefined
                    }
                  />
                ))}
              </section>
            ))}
          {/* Last, and only on the Overview tab: it is context for everything
              above it, not a finding. The Issues tab is a worklist and this
              is not work. */}
          <NotChecked />
        </div>
        )}
      </div>
    </aside>
  );
}

/// The proportional bar behind a row — the panel's chart.
///
/// Screaming Frog pairs its filter list with a graph; this folds the graph
/// into the list. Each row's ground is filled to its share of the crawl in
/// the row's own severity colour, so the panel reads at a glance the way a
/// bar chart does while staying a list of buttons. Pure CSS, no library.
const BAR_TONE: Record<string, string> = {
  critical: "bg-critical-dim",
  warning: "bg-warning-dim",
  notice: "bg-notice-dim",
};

function Bar({ share, tone }: { share?: number; tone: string }) {
  if (!share || share <= 0) return null;
  return (
    <span
      aria-hidden
      className={`pointer-events-none absolute inset-y-0.5 left-0 rounded-r ${tone}`}
      style={{ width: `${Math.min(share, 1) * 100}%` }}
    />
  );
}

function Row({
  row,
  active,
  onClick,
}: {
  row: Line;
  active: boolean;
  onClick?: () => void;
}) {
  const sev = row.severity ? severity(row.severity) : null;
  // A rule that found nothing is drawn quietly but is still drawn: "Missing 0"
  // is the check reporting a pass, and it is not the same as silence.
  const empty = row.count === 0 || row.count === undefined;

  const body = (
    <>
      {sev && (
        <span
          aria-hidden
          className={`shrink-0 ${empty ? "text-fg-faint/50" : sev.tone}`}
        >
          {sev.icon}
        </span>
      )}
      <span
        className={`line-clamp-2 min-w-0 flex-1 ${empty ? "text-fg-faint" : ""}`}
        title={row.label}
      >
        {row.label}
      </span>
      <span className={`nums shrink-0 ${empty ? "text-fg-faint" : "text-fg"}`}>
        {row.count === undefined ? "" : row.count.toLocaleString()}
      </span>
      <span className="nums w-12 shrink-0 text-right text-xs text-fg-faint">
        {row.share === undefined || row.count === 0
          ? ""
          : `${(row.share * 100).toFixed(1)}%`}
      </span>
    </>
  );

  const bar = (
    <Bar
      share={row.share}
      tone={(sev && BAR_TONE[row.severity!]) || "bg-accent-dim"}
    />
  );

  if (!onClick) {
    return (
      <div
        className={`relative flex items-center gap-2 overflow-hidden rounded-sm px-2 py-1 text-sm text-fg-muted ${row.indent ? "pl-4" : ""}`}
      >
        {bar}
        {body}
      </div>
    );
  }

  return (
    <button
      onClick={onClick}
      aria-pressed={active}
      className={`btn relative w-full min-w-0 justify-start overflow-hidden border-transparent bg-transparent text-left text-sm shadow-none ${
        row.indent ? "pl-4" : ""
      } ${active ? "" : "hover:border-border hover:bg-raised"}`}
    >
      {bar}
      {body}
    </button>
  );
}
