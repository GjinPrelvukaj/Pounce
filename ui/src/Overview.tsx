import type { CrawlOverview, IssueOverview, RuleInfo } from "./engine";
import type { FilterState } from "./Filters";
import { NO_FILTERS } from "./Filters";
import { severity } from "./severity";
import type { IssueSelection } from "./Issues";

/// One line of the panel: a label, a count, and the thing clicking it does.
export type Line = {
  label: string;
  count: number;
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
  for (const row of issues?.byRule ?? []) counts.set(row.ruleId, row.urls);

  return [...rules.values()]
    .filter((r) => batches.includes(r.id.split(".")[0]!))
    .map((r) => ({
      label: r.description,
      count: counts.get(r.id) ?? 0,
      share: total > 0 ? (counts.get(r.id) ?? 0) / total : 0,
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
  selection,
  activeFilters,
  onSelectRule,
  onFilter,
}: {
  title: string;
  groups: { title: string; rows: Line[] }[];
  selection: IssueSelection;
  /// The filter bar's current state, so a line already applied reads as applied.
  activeFilters: string;
  onSelectRule: (next: IssueSelection) => void;
  onFilter: (filters: FilterState) => void;
}) {
  return (
    <aside className="flex w-[22rem] min-w-0 shrink-0 flex-col border-l border-border bg-surface">
      <div className="flex shrink-0 items-baseline justify-between gap-2 border-b border-border px-4 py-2.5">
        <h2 className="text-sm font-medium text-fg">{title}</h2>
        <span className="nums text-xs text-fg-faint">URLs · % of total</span>
      </div>

      <div className="min-h-0 flex-1 overflow-auto p-3">
        <div className="flex flex-col gap-3">
          {groups
            .filter((group) => group.rows.length > 0)
            .map((group) => (
              <section key={group.title} className="flex flex-col gap-0.5">
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
        </div>
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
  const empty = row.count === 0;

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
        className={`min-w-0 flex-1 truncate ${empty ? "text-fg-faint" : ""}`}
        title={row.label}
      >
        {row.label}
      </span>
      <span className={`nums shrink-0 ${empty ? "text-fg-faint" : "text-fg"}`}>
        {row.count.toLocaleString()}
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
