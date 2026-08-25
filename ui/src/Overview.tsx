import type { CrawlOverview, RuleInfo, IssueOverview } from "./engine";
import type { FilterState } from "./Filters";
import { NO_FILTERS } from "./Filters";
import { IssueList, type IssueSelection } from "./Issues";

/// One line of the tree: a label, a count, and — where one exists — the filter
/// that shows those rows.
///
/// Screaming Frog's overview panel is the part of it worth copying: a column of
/// counts is the fastest way to learn what a site is made of, and every count
/// in it is a way into the table. A row with no `filters` is a heading or a
/// number the engine cannot express as a query, and it is drawn as text rather
/// than as a dead button.
type Line = {
  label: string;
  count: number;
  filters?: FilterState;
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
  "Informational — 1xx",
  "Worked — 2xx",
  "Redirected — 3xx",
  "Not found — 4xx",
  "Server error — 5xx",
];

function lines(overview: CrawlOverview): { title: string; rows: Line[] }[] {
  return [
    {
      title: "Summary",
      rows: [
        { label: "Pages crawled", count: overview.crawled, filters: NO_FILTERS },
        // Neither of these is in `pages`, so neither can filter the grid:
        // a URL still queued has no row, and one that never answered has no
        // row either. Saying so with a plain number beats a button that
        // silently shows nothing.
        { label: "Still to fetch", count: overview.queued },
        { label: "Never answered", count: overview.failed },
      ],
    },
    {
      title: "What was found",
      rows: overview.byKind.map(([kind, count]) => ({
        label: KIND_LABEL[kind] ?? kind,
        count,
        filters: { ...NO_FILTERS, kind: kind as FilterState["kind"] },
        indent: true,
      })),
    },
    {
      title: "How it answered",
      rows: overview.byClass
        .map((count, i) => ({
          label: CLASS_LABEL[i]!,
          count,
          filters: {
            ...NO_FILTERS,
            statusClass: String(i + 1) as FilterState["statusClass"],
          },
          indent: true,
        }))
        // A healthy crawl is all 2xx, and four permanent zeroes make a panel
        // you stop reading.
        .filter((row) => row.count > 0),
    },
    {
      title: "Indexing",
      rows: [
        {
          label: "Indexable",
          count: overview.indexable,
          filters: { ...NO_FILTERS, indexable: "yes" },
          indent: true,
        },
        {
          label: "Not indexable",
          count: overview.noindex,
          filters: { ...NO_FILTERS, indexable: "no" },
          indent: true,
        },
      ],
    },
  ];
}

/// The right-hand panel: what the crawl contains, and what is wrong with it.
///
/// Two tabs over one crawl rather than two panels, because they answer
/// different questions about the same rows and only one of them is ever the
/// question you have.
export function Overview({
  tab,
  onTab,
  overview,
  issues,
  rules,
  selection,
  live,
  onSelectIssue,
  onFilter,
  active,
}: {
  tab: "overview" | "issues";
  onTab: (tab: "overview" | "issues") => void;
  overview: CrawlOverview | null;
  issues: IssueOverview | null;
  rules: Map<string, RuleInfo>;
  selection: IssueSelection;
  live: boolean;
  onSelectIssue: (next: IssueSelection) => void;
  onFilter: (filters: FilterState) => void;
  /// The filter bar's current state, so a line that is already applied reads
  /// as applied.
  active: string;
}) {
  return (
    <aside className="flex w-80 min-w-0 shrink-0 flex-col border-l border-border bg-surface">
      <div className="flex shrink-0 gap-1 border-b border-border px-2 pt-2">
        {(["overview", "issues"] as const).map((name) => (
          <button
            key={name}
            onClick={() => onTab(name)}
            aria-pressed={tab === name}
            className="btn rounded-b-none border-transparent bg-transparent aria-pressed:border-border aria-pressed:border-b-transparent aria-pressed:bg-canvas"
          >
            {name === "overview" ? "Overview" : "What to fix"}
          </button>
        ))}
      </div>

      <div className="min-h-0 flex-1 overflow-auto p-3">
        {tab === "overview" ? (
          overview === null ? null : (
            <div className="flex flex-col gap-3">
              {lines(overview)
                .filter((group) => group.rows.length > 0)
                .map((group) => (
                  <section key={group.title} className="flex flex-col gap-0.5">
                    <h3 className="text-sm text-fg-faint">{group.title}</h3>
                    {group.rows.map((row) => (
                      <Row
                        key={row.label}
                        row={row}
                        total={overview.crawled}
                        active={
                          row.filters !== undefined &&
                          JSON.stringify(row.filters) === active
                        }
                        onFilter={onFilter}
                      />
                    ))}
                  </section>
                ))}
            </div>
          )
        ) : issues === null ? null : (
          <IssueList
            overview={issues}
            rules={rules}
            selection={selection}
            live={live}
            onSelect={onSelectIssue}
          />
        )}
      </div>
    </aside>
  );
}

function Row({
  row,
  total,
  active,
  onFilter,
}: {
  row: Line;
  total: number;
  active: boolean;
  onFilter: (filters: FilterState) => void;
}) {
  // Percentages of nothing are not zero, they are absent.
  const share =
    total > 0 && row.count > 0 ? `${((row.count / total) * 100).toFixed(1)}%` : "";
  const body = (
    <>
      <span className="min-w-0 flex-1 truncate">{row.label}</span>
      <span className="tabular shrink-0 text-fg">{row.count.toLocaleString()}</span>
      <span className="tabular w-12 shrink-0 text-right text-xs text-fg-faint">
        {share}
      </span>
    </>
  );

  if (!row.filters) {
    return (
      <div
        className={`flex items-center gap-2 px-2 py-1 text-md text-fg-muted ${row.indent ? "pl-4" : ""}`}
      >
        {body}
      </div>
    );
  }

  return (
    <button
      onClick={() => onFilter(row.filters!)}
      aria-pressed={active}
      className={`btn w-full min-w-0 justify-start border-transparent bg-transparent text-left text-md ${
        row.indent ? "pl-4" : ""
      } ${active ? "" : "hover:border-border hover:bg-raised"}`}
    >
      {body}
    </button>
  );
}
