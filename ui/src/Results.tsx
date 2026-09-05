import { useEffect, useMemo, useRef, useState } from "react";
import { Detail } from "./Detail";
import {
  FilterBar,
  NO_FILTERS,
  toFilters,
  type FilterState,
} from "./Filters";
import { COLUMNS, Grid, IMAGE_COLUMNS, SITEMAP_COLUMNS } from "./Grid";
import { Tree } from "./Tree";
import { DEFAULT_COLUMNS, saveColumns, storedColumns, toggleColumn } from "./columns";
import { selectionFilters, type IssueSelection } from "./Issues";
import type { Command } from "./CommandPalette";
import { ago, basename, type Recent } from "./recents";
import { Overview, ruleLines, summaryLines } from "./Overview";
import type { Line } from "./Overview";
import * as Menu from "@radix-ui/react-dropdown-menu";
import { Group, Panel, Separator, useDefaultLayout } from "react-resizable-panels";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import {
  crawlOverview,
  exportRows,
  issueOverview,
  resourceRows,
  sitemapSummary,
  sitemapUrls,
  supportedSorts,
  type CrawlHandle,
  type CrawlOverview,
  type IssueOverview,
  type ProgressEvent,
  type RuleInfo,
  type ResourceRow,
  type SitemapSummary,
  type SitemapUrl,
  type RowView,
  type SortColumn,
} from "./engine";

/// A crawl in flight: the file being written and the last tick that described
/// it.
export type Live = { path: string; progress: ProgressEvent };

/// Views over one crawl.
///
/// This is the shape worth taking from Screaming Frog: a tab is not a screen,
/// it is **a saved question with its own columns**. "Page titles" is the same
/// rows as "All pages" with the title, its length and the word count brought
/// forward and the bytes dropped, because when you are auditing titles those
/// are the four things you look at.
///
/// Each one is a `FilterState` the engine already serves plus a column list the
/// grid already knows, so switching tabs is a query and a re-render — no mode,
/// no second code path.
/// The columns a finding needs you to see.
///
/// Clicking "More than one page uses this meta description" and being shown a
/// Title column is the interface hiding the evidence for its own claim. Keyed
/// by rule batch, because every rule in a batch is about the same field.
///
/// This was reported as a false positive in the duplicate-description rule:
/// four New Jersey townships are called Franklin, the site templates its
/// description on town name alone, and all four pages carry byte-identical
/// text. The rule was right. The grid was showing Title.
const FOCUS: Record<string, string[]> = {
  title: ["row", "status", "url", "title", "titleLength"],
  description: ["row", "status", "url", "metaDescription", "descriptionLength"],
  indexability: ["row", "status", "url", "canonical", "noindex"],
  content: ["row", "status", "url", "h1", "h1Count", "wordCount"],
  response: ["row", "status", "url", "title", "elapsedMs"],
  media: ["row", "status", "url", "kind", "size"],
  links: ["row", "status", "url", "title", "depth"],
};

/// The order that puts a duplicate next to its partner.
///
/// Only the two rules whose evidence is a sortable column. `content.duplicate-body`
/// is absent: it groups by `body_hash`, which is not a grid column and would
/// mean a fifteenth index to make one row of the panel behave.
const GROUP_BY: Record<string, SortColumn> = {
  "title.duplicate": "title",
  "description.duplicate": "metaDescription",
};

const VIEWS: {
  id: string;
  label: string;
  filters: FilterState;
  columns: string[];
  /// The rule batches this view's panel enumerates. `title` gives the Page
  /// Titles panel its Missing / Duplicate / Too long rows — the rules *are*
  /// the filter list, which is the thing Screaming Frog's right panel is and
  /// the first version of this panel was not.
  batches: string[];
  /// `summary` adds the crawl-composition rows above the rules.
  panel?: "summary";
}[] = [
  {
    id: "all",
    label: "All pages",
    filters: NO_FILTERS,
    columns: ["row", "status", "url", "title", "wordCount", "depth", "size"],
    batches: [],
    panel: "summary",
  },
  {
    id: "urls",
    label: "URLs",
    // Not narrowed to pages: an image with a 200-character address is the same
    // problem as a page with one, and the address is the whole subject here.
    filters: NO_FILTERS,
    columns: ["row", "status", "url", "urlLength", "urlParams", "urlNotes"],
    // No rules to enumerate — the registry is capped at thirty and none of
    // them is about the address itself. The panel shows what the crawl
    // contains instead of an empty list.
    batches: [],
    panel: "summary",
  },
  {
    id: "titles",
    label: "Page titles",
    filters: { ...NO_FILTERS, kind: "html" },
    columns: ["row", "status", "url", "title", "titleLength", "wordCount"],
    batches: ["title"],
  },
  {
    id: "descriptions",
    label: "Meta descriptions",
    filters: { ...NO_FILTERS, kind: "html" },
    columns: ["row", "status", "url", "metaDescription", "descriptionLength"],
    batches: ["description"],
  },
  {
    id: "headings",
    label: "Headings",
    filters: { ...NO_FILTERS, kind: "html" },
    columns: ["row", "status", "url", "h1", "h1Count", "h2Count", "wordCount"],
    batches: ["content"],
  },
  {
    id: "duplicates",
    label: "Duplicates",
    filters: { ...NO_FILTERS, kind: "html" },
    columns: ["row", "status", "url", "title", "metaDescription"],
    // Three rules from three batches, listed by id rather than by batch. Two
    // pages with the same title and two with the same description are the same
    // conversation with a client, and they were three clicks apart.
    batches: [
      "title.duplicate",
      "description.duplicate",
      "content.duplicate-body",
    ],
  },
  {
    id: "canonicals",
    label: "Canonicals",
    filters: { ...NO_FILTERS, kind: "html" },
    columns: ["row", "status", "url", "canonical", "noindex"],
    batches: ["indexability"],
  },
  {
    id: "broken",
    label: "Broken",
    filters: { ...NO_FILTERS, statusClass: "bad" },
    columns: ["row", "status", "url", "title", "depth"],
    batches: ["response"],
  },
  {
    id: "redirects",
    label: "Redirects",
    filters: { ...NO_FILTERS, statusClass: "3" },
    columns: ["row", "status", "url", "title", "depth"],
    batches: ["response"],
  },
  {
    id: "noindex",
    label: "Not indexable",
    filters: { ...NO_FILTERS, indexable: "no" },
    columns: ["row", "status", "url", "title", "canonical"],
    batches: ["indexability"],
  },
  {
    id: "sitemap",
    label: "Sitemap",
    // Its own source, like Images: these rows are what the *site* says it has,
    // not what the crawl found, and the two disagreeing is the point.
    filters: NO_FILTERS,
    columns: ["row", "url", "status", "source"],
    batches: [],
  },
  {
    id: "images",
    label: "Images",
    // No filters, because this view does not read `pages` at all. Images are
    // `resources` — checked with HEAD, no body, no title — and the old
    // `kind: "image"` filter asked `pages` for a kind the crawler stopped
    // writing when migration 011 gave them their own table. It matched
    // nothing, on every crawl, while the panel beside it counted their
    // findings.
    filters: NO_FILTERS,
    columns: ["row", "status", "url", "contentLength", "contentType", "issues"],
    batches: ["media"],
  },
  {
    id: "slowest",
    label: "Response times",
    filters: NO_FILTERS,
    columns: ["row", "status", "url", "elapsedMs", "size"],
    batches: ["response"],
    panel: "summary",
  },
];

/// The panel's index, grouped by aspect — every check in the build, always.
///
/// **This is the correction the 2026-09-05 teardown found.** Screaming Frog's
/// right panel is not scoped to the tab you are standing on: it lists every
/// aspect at once, and clicking a line *takes you to that tab* with the filter
/// already set. That is what makes it navigation rather than a report, and it
/// is how the whole application is driven.
///
/// Pounce's panel enumerated only the current view's rules, so a finding about
/// images was invisible from Page Titles and the panel could filter but never
/// move. `view` on every line is the missing half.
///
/// Batches map to a home view. Several views share a batch — Broken, Redirects
/// and Response times all read `response` — so the index is keyed by batch and
/// names one home for each, rather than one group per view repeating the rules.
const INDEX: { title: string; view: string; all: string; batches: string[] }[] = [
  { title: "Page titles", view: "titles", all: "All page titles", batches: ["title"] },
  { title: "Meta descriptions", view: "descriptions", all: "All meta descriptions", batches: ["description"] },
  { title: "Content and headings", view: "headings", all: "All content", batches: ["content"] },
  { title: "Indexability", view: "canonicals", all: "All canonicals", batches: ["indexability"] },
  { title: "Response codes", view: "broken", all: "Everything that broke", batches: ["response"] },
  { title: "Images", view: "images", all: "All images", batches: ["media"] },
  { title: "Links", view: "all", all: "All pages", batches: ["links"] },
];

/// The results screen: findings on the left as navigation, one crawl's rows on
/// the right, the selected row underneath.
export function Results({
  handle,
  live,
  rules,
  recent,
  pace,
  onOpenRecent,
  onForgetRecent,
  onCommands,
}: {
  /// `null` before any crawl is open. The screen still renders in full: this
  /// is the difference between an application you can read at rest and one
  /// that hides behind a welcome page.
  handle: CrawlHandle | null;
  live: Live | null;
  rules: Map<string, RuleInfo>;
  recent: Recent[];
  pace: { text: string; heavy: boolean } | null;
  onOpenRecent: (path: string) => void;
  onForgetRecent: (path: string) => void;
  /// Publishes this screen's commands to the global palette.
  onCommands: (commands: Command[]) => void;
}) {
  const [overview, setOverview] = useState<IssueOverview | null>(null);
  const [contents, setContents] = useState<CrawlOverview | null>(null);
  // What the site says it has, for the Sitemap tab's panel. Fetched with the
  // overview rather than on tab switch: it is two counts and a file, and a
  // panel that fills in a beat after you arrive reads as broken.
  const [sitemap, setSitemap] = useState<SitemapSummary | null>(null);
  const [total, setTotal] = useState(0);
  // Which finding the grid is filtered to. Held here rather than in the grid
  // because the rail and the grid are two views of one selection.
  const [selection, setSelection] = useState<IssueSelection>(null);
  // Bumped once a second while a crawl writes, which is what tells the grid
  // its cached windows describe an older state of the same file.
  const [refreshKey, setRefreshKey] = useState(0);
  const [bar, setBar] = useState<FilterState>(NO_FILTERS);
  const [sort, setSort] = useState<SortColumn>("url");
  const [direction, setDirection] = useState<"asc" | "desc">("asc");
  // Which sorts the engine will run against these filters. `undefined` while
  // the answer is in flight, which is not the same as "none" — greying every
  // header for a moment on each keystroke would be worse than a brief guess.
  const [sorts, setSorts] = useState<SortColumn[] | undefined>(undefined);
  // The row the detail pane is showing. An id, not a row: the pane fetches
  // everything itself, and a row object here would go stale the moment the
  // crawl rewrote that page.
  const [opened, setOpened] = useState<number | null>(null);
  // Seeded from what this person last chose, then owned by whichever view is
  // selected. The picker still writes the preference; a view does not, because
  // "I looked at titles once" is not a preference.
  const [columns, setColumns] = useState<string[]>(storedColumns);
  const gridFocus = useRef<(() => void) | null>(null);
  // What the last export did, shown beside the button. A file written silently
  // is a file the user goes looking for.
  const [exported, setExported] = useState<string | null>(null);
  // List or tree. Two readings of the same crawl, not two modes: the tree
  // answers "what shape is this site", the table answers "which pages", and
  // clicking a page in one opens it in the pane the other uses.
  const [shape, setShape] = useState<"list" | "tree">("list");

  async function exportView() {
    // What the grid is showing. The report formats ignore it — a client report
    // is about the crawl, not about the tab someone left open — but a CSV of
    // "this view" has to be this view.
    const subject =
      view?.id === "images" ? "images" : view?.id === "sitemap" ? "sitemap" : "pages";
    const chosen = await saveDialog({
      // Excel first, and the default, because it is the one most people want
      // and the only one of the three that is a document rather than a
      // transfer format. CSV and JSON stay for the pipelines that read them.
      defaultPath: "pounce-report.xlsx",
      filters: [
        { name: "PDF report", extensions: ["pdf"] },
        { name: "Word report", extensions: ["docx"] },
        { name: "Excel workbook", extensions: ["xlsx"] },
        { name: "CSV", extensions: ["csv"] },
        { name: "JSON", extensions: ["json"] },
      ],
    });
    if (typeof chosen !== "string") return;
    setExported("Writing…");
    try {
      const rows = await exportRows({
        path: chosen,
        filters,
        sort,
        direction,
        subject,
        // Written by the browser, which is the only part of this that knows
        // where the reader is and how they write a date.
        today: new Date().toLocaleDateString(undefined, {
          day: "numeric",
          month: "long",
          year: "numeric",
        }),
      });
      const name = chosen.split("/").pop();
      setExported(
        chosen.endsWith(".pdf") || chosen.endsWith(".docx")
          ? `${rows.toLocaleString()} findings → ${name}`
          : chosen.endsWith(".xlsx")
            ? `${rows.toLocaleString()} rows, plus findings, images and sitemap → ${name}`
            : `${rows.toLocaleString()} rows → ${name}`,
      );
    } catch (e) {
      const api = e as { message?: string };
      setExported(api?.message ?? String(e));
    }
  }

  // When a finding is selected, show the columns it is about. Skipped for the
  // "any issue" selection, which is not about one field.
  useEffect(() => {
    if (selection === null || selection === "*") return;
    const batch = selection.split(".")[0] ?? "";
    const focus = FOCUS[batch];
    if (focus) setColumns(focus);
    // A finding about duplication is only readable when the duplicates are
    // next to each other, and the grid's default order is by URL — which is
    // the one order that pulls them apart, since a duplicate description is
    // most often two pages in different sections. Sorting by the field itself
    // is what makes four identical rows look like four identical rows.
    const group = GROUP_BY[selection];
    if (group) {
      setSort(group);
      setDirection("asc");
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selection]);

  const filters = useMemo(
    () => [...selectionFilters(selection), ...toFilters(bar)],
    [selection, bar],
  );
  const filterKey = JSON.stringify(filters);
  const barKey = JSON.stringify(bar);
  // A view is its filters and its columns together: the same rows with a
  // different projection is a different question.
  //
  // **Except the search box.** Typing in it used to drop the tab to "Custom
  // view" — the underline went out and the panel changed under you, for
  // narrowing the list you were already looking at. A search is a refinement
  // *within* a view, not a different one, which is why Screaming Frog puts its
  // search above the tabs and keeps it there as you move between them. So the
  // identity of a view ignores `urlContains` on both sides.
  const identity = (f: FilterState) =>
    JSON.stringify({ ...f, urlContains: "" });
  const view = VIEWS.find(
    (v) =>
      identity(v.filters) === identity(bar) &&
      JSON.stringify(v.columns) === JSON.stringify(columns),
  );

  // **The other half of "why does it say Custom".** A view is its filters *and*
  // its columns, so ticking one extra column un-names the tab you are standing
  // on — and the name vanishing right after a search is what made this look
  // like a search bug. It is not: the search is a refinement and is ignored
  // above, but a column change is real and the tab genuinely is no longer that
  // view. Naming it anyway, with what changed, beats "Custom view" — which
  // tells a reader nothing about where they are or how to get back.
  const nearest = VIEWS.find((v) => identity(v.filters) === identity(bar));
  const viewLabel = view
    ? view.label
    : nearest
      ? `${nearest.label} · your columns`
      : "Custom view";

  // `url` is the fallback because it is the one column supported against every
  // filter shape this build has — a substring filter is *only* offered with it.
  useEffect(() => {
    supportedSorts(filters)
      .then((allowed) => {
        setSorts(allowed);
        if (!allowed.includes(sort)) {
          setSort("url");
          setDirection("asc");
        }
      })
      .catch(() => setSorts(undefined));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [filterKey]);

  // One overview query at a time. The count is a `GROUP BY` over every issue in
  // the file, so on a large crawl it can take longer than the second between
  // ticks — and queuing them would put the reader in the writer's way rather
  // than out of it.
  const overviewBusy = useRef(false);
  function refreshOverview() {
    if (overviewBusy.current) return;
    overviewBusy.current = true;
    // One flight for both panels. They are read together and neither is worth
    // a second round trip on its own.
    Promise.all([issueOverview(), crawlOverview(), sitemapSummary()])
      .then(([issues, contents, maps]) => {
        setOverview(issues);
        setContents(contents);
        setSitemap(maps);
      })
      .catch(() => {})
      .finally(() => {
        overviewBusy.current = false;
      });
  }

  useEffect(() => {
    if (handle) refreshOverview();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [handle?.path]);

  // Results while the crawl runs, ticked once a second rather than at the
  // progress rate: 10 Hz of `count(*)` over a growing table is the reader
  // competing with the writer.
  const liveSecond = live ? Math.floor(live.progress.elapsedMs / 1000) : 0;
  useEffect(() => {
    if (!live) return;
    setRefreshKey((k) => k + 1);
    refreshOverview();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [liveSecond]);

  const pages = live ? live.progress.written : (handle?.pages ?? 0);

  // Pane sizes are a preference, so they persist. `useDefaultLayout` reads and
  // writes localStorage and hands back the props the Group needs.
  const hLayout = useDefaultLayout({ id: "pounce.layout.h", storage: localStorage });
  const vLayout = useDefaultLayout({ id: "pounce.layout.v", storage: localStorage });

  // Publish this screen's commands to the palette. The views are static; the
  // findings depend on what the crawl actually contains, so a rule with no
  // occurrences is offered with a `0` beside it rather than hidden — the same
  // reasoning as the panel, where a check reporting a pass is information.
  useEffect(() => {
    const views: Command[] = VIEWS.map((v) => ({
      id: `view-${v.id}`,
      label: v.label,
      group: "Views",
      run: () => {
        setBar(v.filters);
        setColumns(v.columns);
        if (v.id === "slowest") {
          setSort("elapsedMs");
          setDirection("desc");
        }
      },
    }));
    const counts = new Map<string, number>();
    for (const row of overview?.byRule ?? []) counts.set(row.ruleId, row.urls);
    const findings: Command[] = [...rules.values()].map((r) => ({
      id: `rule-${r.id}`,
      label: r.description,
      group: "Findings",
      hint: (counts.get(r.id) ?? 0).toLocaleString(),
      run: () => setSelection(r.id),
    }));
    onCommands([
      ...views,
      ...findings,
      {
        id: "export",
        label: "Export this view…",
        group: "Actions",
        run: () => void exportView(),
      },
      {
        id: "tree",
        label: "Show the site as a folder tree",
        group: "Actions",
        run: () => setShape("tree"),
      },
      {
        id: "list",
        label: "Show the site as a table",
        group: "Actions",
        run: () => setShape("list"),
      },
      {
        id: "clear",
        label: "Clear all filters",
        group: "Actions",
        run: () => {
          setBar(NO_FILTERS);
          setSelection(null);
        },
      },
    ]);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [overview, rules, onCommands]);

  // What the right panel enumerates for the view that is open. The whole-crawl
  // rows only appear on the views that are about the whole crawl; everywhere
  // else the panel is the rules for that aspect, zeroes included.
  // Before a crawl the panel shows the same tree with zeros rather than
  // nothing. A zero is a fact about this crawl; an absent row teaches nobody
  // what the panel is for.
  const ZEROED: CrawlOverview = {
    crawled: 0,
    queued: 0,
    failed: 0,
    byKind: [["html", 0]],
    byClass: [0, 0, 0, 0, 0],
    indexable: 0,
    noindex: 0,
    // A crawl that does not exist has not been analysed, and the banner is
    // suppressed before one is open — see `interrupted` below.
    analysed: false,
    jsShell: 0,
  };
  // The Sitemap tab answers a different question from every other tab, so its
  // panel is a different panel: not "which of these checks fired" but "do
  // these two lists agree". Both disagreements are findings; neither is a
  // rule, because a rule fires on a page and these fire on the *gap* between
  // what was crawled and what was declared.
  const sitemapGroups = [
    {
      title: "What the site declares",
      rows: [
        {
          label: "URLs in the sitemap",
          count: sitemap?.urls ?? 0,
          share: 1,
        },
        {
          label: "Sitemap files read",
          count: sitemap?.files ?? 0,
        },
        {
          // A statement, not a count. It carries no number because there is
          // no number: the file either answered or it did not.
          label:
            sitemap?.robotsStatus === undefined || sitemap?.robotsStatus === null
              ? "robots.txt was not read"
              : `robots.txt answered ${sitemap.robotsStatus}`,
        },
      ],
    },
    {
      title: "Where the two disagree",
      // Only when there is something to disagree with. With no sitemap found,
      // every page is "missing from the sitemap" — true of the arithmetic and
      // false of the site, and it would be the loudest number on the panel.
      rows: (sitemap?.files ?? 0) === 0 ? [] : [
        {
          label: "Listed in the sitemap, not reached by any link",
          count: sitemap?.notCrawled ?? 0,
          share:
            sitemap && sitemap.urls > 0
              ? sitemap.notCrawled / sitemap.urls
              : undefined,
          indent: true,
        },
        // `null` means a sitemap was too large to read in full. No number at
        // all then — every page past the cap would count as one the site
        // forgot to list, which is a finding invented by our own limit.
        sitemap?.notListed === null
          ? {
              label:
                "A sitemap was too large to read in full, so pages missing from it cannot be counted",
              indent: true,
            }
          : {
              label: "Crawled and indexable, missing from the sitemap",
              count: sitemap?.notListed ?? 0,
              share:
                pages > 0 ? (sitemap?.notListed ?? 0) / pages : undefined,
              indent: true,
            },
      ],
    },
  ];

  // A crawl that was stopped keeps its pages and has none of its conclusions:
  // no duplicate detection, no orphan pages, no broken internal links, no
  // sitemap comparison. Every one of those reads as "nothing found" instead of
  // "never looked", which is the failure the "not checked" list exists to
  // prevent — arrived at from the other direction.
  const interrupted = handle !== null && live === null && contents?.analysed === false;

  // Global, not per-view: the same index whichever tab is open, because it is
  // the thing you navigate *with*. `current` no longer decides what the panel
  // contains — only which of its lines reads as active.
  const panelGroups = [
    ...summaryLines(contents ?? ZEROED),
    {
      title: "What to fix",
      rows: [
        {
          label: "Every page with something to fix",
          // Pages, not URLs: the row says "page", and clicking it filters
          // `pages.has_issue`. Counting the image findings here made the
          // headline disagree with the list it opens — 39 of 18 pages, at
          // 216.7% of a crawl.
          count: overview?.pagesWithIssues ?? 0,
          share:
            overview && pages > 0 ? overview.pagesWithIssues / pages : undefined,
          rule: "*",
        },
      ],
    },
    ...INDEX.map((group) => ({
      title: group.title,
      rows: [
        // No count on the "All" row, deliberately. Screaming Frog can put one
        // there because every tab is a filter over one table; three of ours
        // read different tables entirely, and this panel has already shipped
        // "18 pages" over a list of 88 images and a finding at 216% of a
        // crawl. A row that navigates and does not claim a number is worth
        // more than a row that claims the wrong one.
        { label: group.all, view: group.view },
        ...ruleLines(group.batches, rules, overview, pages).map((row) => ({
          ...row,
          view: group.view,
        })),
      ],
    })),
    ...sitemapGroups.map((group) => ({
      ...group,
      rows: group.rows.map((row) => ({ ...row, view: "sitemap" })),
    })),
  ];

  /// One click: go to the aspect, set the filter, select the rule.
  const pick = (row: Line) => {
    // Clicking the selected rule again clears it and stays put. Moving the
    // view on the way *out* of a selection would make the clear feel like a
    // navigation someone did not ask for.
    if (row.rule !== undefined && selection === row.rule) {
      setSelection(null);
      return;
    }
    const target = row.view ? VIEWS.find((v) => v.id === row.view) : undefined;
    if (target) {
      setBar(target.filters);
      setColumns(target.columns);
      if (target.id === "slowest") {
        setSort("elapsedMs");
        setDirection("desc");
      }
    } else if (row.filters) {
      setBar(row.filters);
    }
    setSelection(row.rule ?? null);
  };


  return (
    // Draggable splitters, which Screaming Frog has and we did not. Hand
    // rolling one is a pointer-capture, keyboard and persistence problem, so
    // this is `react-resizable-panels`: it ships arrow-key resizing on a
    // focused handle and `autoSaveId` remembers the layout per person.
    <Group orientation="horizontal" className="flex min-h-0 flex-1" {...hLayout}>
      <Panel id="main" defaultSize="78%" minSize="45%" className="flex min-w-0 flex-col">
        <main className="flex min-h-0 min-w-0 flex-1 flex-col">
        {/* Above the tabs, because it changes what every one of them means.
            A stopped crawl keeps its pages and none of its conclusions, and
            an empty Duplicates or Sitemap tab then reads as a clean result
            rather than as work that never ran. */}
        {interrupted && (
          <p
            role="status"
            className="flex items-baseline gap-2 border-b border-border bg-warning-dim px-3 py-2 text-sm text-fg"
          >
            <span aria-hidden className="text-warning">
              ▲
            </span>
            <span>
              <span className="font-medium">This crawl was stopped before it finished.</span>{" "}
              Its pages are complete, but the checks that run at the end did not:
              duplicate titles and descriptions, orphan pages, broken internal
              links, and the sitemap comparison. Crawl the site again to get
              them.
            </span>
          </p>
        )}
        <div className="flex flex-wrap items-center gap-1 border-b border-border px-3 pt-2">
          {VIEWS.map((v) => (
            <button
              key={v.id}
              onClick={() => {
                setBar(v.filters);
                setColumns(v.columns);
                // Response times is the one view with an opinion about order:
                // it exists to answer "what is slow", and that is a sort.
                if (v.id === "slowest") {
                  setSort("elapsedMs");
                  setDirection("desc");
                }
              }}
              // The tab stays lit when only the *columns* differ. It used to
              // go dark and hand its place to a phantom "Custom" tab — an
              // unclickable `span` with no way back — which is what made a
              // column change, or a search landing next to one, look like the
              // app had jumped somewhere else.
              aria-pressed={view?.id === v.id || (!view && nearest?.id === v.id)}
              className="tab"
            >
              {v.label}
            </button>
          ))}
          {!view && (
            <button
              onClick={() => {
                // Back to the view you are standing on, or to the first one.
                const target = nearest ?? VIEWS[0]!;
                setBar(target.filters);
                setColumns(target.columns);
              }}
              title={
                nearest
                  ? `Back to ${nearest.label}'s columns`
                  : "Back to All pages"
              }
              className="btn ml-1 shrink-0 px-2 text-xs"
            >
              {nearest ? "Your columns · reset" : "Custom · reset"}
            </button>
          )}
        </div>

        {/* The filters wrap; the two buttons do not. Letting the whole row wrap
            put "Columns" on a line of its own on a 1,280px window, which reads
            as a control that belongs to nothing. */}
        <div className="flex items-start gap-2 px-3 py-2">
          <div className="min-w-0 flex-1">
            <FilterBar value={bar} onChange={setBar} />
          </div>
          {exported && (
            <span className="nums shrink-0 py-1 text-sm text-fg-muted">
              {exported}
            </span>
          )}
          <div className="flex shrink-0 gap-1">
            {(["list", "tree"] as const).map((option) => (
              <button
                key={option}
                onClick={() => setShape(option)}
                aria-pressed={shape === option}
                className="tab"
              >
                {option === "list" ? "List" : "Tree"}
              </button>
            ))}
          </div>
          <button onClick={() => void exportView()} className="btn shrink-0">
            Export…
          </button>
          {/* A menu, not a modal. Toggling a column is a two-second decision
              you make while looking at the table, and a modal that covers the
              table to change what the table shows is the wrong shape for it.
              Radix supplies the focus handling, the typeahead and Escape. */}
          <Menu.Root>
            <Menu.Trigger className="btn shrink-0">Columns</Menu.Trigger>
            <Menu.Portal>
              <Menu.Content className="menu" sideOffset={6} align="end">
                <Menu.Label className="menu-label">Show columns</Menu.Label>
                {COLUMNS.map((column) => (
                  <Menu.CheckboxItem
                    key={column.key}
                    className="menu-item"
                    checked={columns.includes(column.key as string)}
                    // Radix closes on select by default; a column picker is a
                    // list you tick several things in.
                    onSelect={(e) => e.preventDefault()}
                    onCheckedChange={() => {
                      const next = toggleColumn(columns, column.key as string);
                      setColumns(next);
                      saveColumns(next);
                    }}
                  >
                    <span className="w-3 shrink-0 text-accent-fg">
                      {columns.includes(column.key as string) ? "\u2713" : ""}
                    </span>
                    {column.header}
                  </Menu.CheckboxItem>
                ))}
                <Menu.Separator className="my-1 h-px bg-border" />
                <Menu.Item
                  className="menu-item"
                  onSelect={() => {
                    setColumns(DEFAULT_COLUMNS);
                    saveColumns(DEFAULT_COLUMNS);
                  }}
                >
                  <span className="w-3 shrink-0" />
                  Reset to defaults
                </Menu.Item>
              </Menu.Content>
            </Menu.Portal>
          </Menu.Root>
        </div>

        {/* The chrome above (tabs, filters) does not resize; only the grid and
            the pane below it trade height, which is what the splitter is for. */}
        <Group orientation="vertical" className="flex min-h-0 flex-1 flex-col" {...vLayout}>
        <Panel id="rows" defaultSize="68%" minSize="20%" className="flex min-h-0 flex-col">
        {/* The tree is mounted only while it is shown. It holds the folders
            someone opened, and keeping that alive behind the table would mean
            reopening a crawl and finding last crawl's folders still expanded. */}
        {shape === "tree" && (
          <Tree
            // Keyed by the file, so opening a second crawl does not inherit
            // the first one's expanded folders.
            key={handle?.path ?? "none"}
            enabled={handle !== null}
            selectedId={opened}
            refreshKey={refreshKey}
            onOpen={setOpened}
          />
        )}
        {shape === "list" && view?.id === "sitemap" && (
          <Grid<SitemapUrl>
            allColumns={SITEMAP_COLUMNS}
            visible={columns}
            source={(offset, limit) => sitemapUrls({ offset, limit })}
            sourceKey="sitemap"
            filters={[]}
            sort="url"
            direction="asc"
            supportedSorts={[]}
            refreshKey={refreshKey}
            enabled={handle !== null}
            emptyMessage="No sitemap found. Pounce looks for one in robots.txt and then at /sitemap.xml — a site with neither has none to check against."
            onTotal={setTotal}
          />
        )}
        {shape === "list" && view?.id === "images" && (
          <Grid<ResourceRow>
            allColumns={IMAGE_COLUMNS}
            visible={columns}
            source={(offset, limit) => resourceRows({ offset, limit })}
            sourceKey="resources"
            filters={[]}
            sort="url"
            direction="asc"
            supportedSorts={[]}
            refreshKey={refreshKey}
            enabled={handle !== null}
            emptyMessage={
              live
                ? "No images yet. They are checked after the pages that reference them."
                : "This crawl checked no images. Turn on “Check images” in Options before crawling."
            }
            onTotal={setTotal}
          />
        )}
        {shape === "list" && view?.id !== "images" && view?.id !== "sitemap" && (
        <Grid<RowView>
          filters={filters}
          visible={columns}
          sort={sort}
          direction={direction}
          supportedSorts={sorts}
          onSort={(column) => {
            // Clicking the column already sorted reverses it; clicking another
            // starts that one ascending, which is what every table does and the
            // only behaviour nobody has to be told about.
            if (column === sort) {
              setDirection((d) => (d === "asc" ? "desc" : "asc"));
            } else {
              setSort(column);
              setDirection("asc");
            }
          }}
          enabled={handle !== null}
          emptyContent={
            recent.length > 0 ? (
              <div className="flex w-full max-w-lg flex-col gap-1">
                <h3 className="px-1 text-xs font-semibold tracking-[0.07em] text-fg-faint uppercase">
                  Recent crawls
                </h3>
                <ul className="flex flex-col">
                  {recent.map((r) => (
                    <li
                      key={r.path}
                      className="flex items-center gap-2 border-b border-border/60 py-2"
                    >
                      <button
                        onClick={() => onOpenRecent(r.path)}
                        title={r.path}
                        className="focusable nums min-w-0 flex-1 truncate rounded-sm text-left text-sm text-accent-fg hover:underline"
                      >
                        {basename(r.path)}
                      </button>
                      <span className="nums shrink-0 text-xs text-fg-faint">
                        {r.pages.toLocaleString()} pages · {ago(r.openedAt)}
                      </span>
                      <button
                        onClick={() => onForgetRecent(r.path)}
                        aria-label={`Remove ${basename(r.path)} from recent crawls`}
                        className="focusable shrink-0 rounded-sm px-1 text-xs text-fg-faint transition-colors duration-150 ease-state hover:text-critical"
                      >
                        ✕
                      </button>
                    </li>
                  ))}
                </ul>
              </div>
            ) : (
              <p className="max-w-md text-center text-sm text-fg-muted">
                Enter a website address above and press Start. Pounce checks
                thirty things about every page and keeps the whole crawl in a
                file you can reopen.
              </p>
            )
          }
          refreshKey={refreshKey}
          selectedId={opened}
          onOpen={(row) => setOpened(row.id)}
          registerFocus={(focus) => {
            gridFocus.current = focus;
          }}
          emptyMessage={
            live
              ? "No rows yet. Pages reach the file 500 at a time, and the first batch has not landed."
              : filters.length === 0
                ? "This crawl has no pages."
                : "No pages match these filters."
          }
          onClearFilters={
            filters.length > 0
              ? () => {
                  setBar(NO_FILTERS);
                  setSelection(null);
                }
              : undefined
          }
          onTotal={setTotal}
        />
        )}

        {/* The foot of the pane, where Screaming Frog keeps its counts: what
            the grid is showing, out of what, and why it is not showing the
            rest. */}
        <footer className="flex shrink-0 flex-wrap items-center gap-2 border-t border-border bg-surface px-3 py-2">
          <span className="nums text-sm text-fg-muted">
            {/* The Images view counts images. It said "18 pages" under a list
                of 88 images, which is the same mistake as the 216% — a number
                describing something other than what is on screen. */}
            {view?.id === "sitemap"
              ? `${total.toLocaleString()} URLs in the sitemap`
              : view?.id === "images"
              ? `${total.toLocaleString()} images${live ? " so far" : ""}`
              : filters.length === 0
                ? `${pages.toLocaleString()} pages${live ? " so far" : ""}`
                : `Showing ${total.toLocaleString()} of ${pages.toLocaleString()} pages`}
          </span>
          {selection !== null && (
            <>
              <span className="text-sm text-fg-faint">·</span>
              <span className="text-sm text-fg">
                {selection === "*"
                  ? "every page with something to fix"
                  : (rules.get(selection)?.description ?? selection)}
              </span>
              <button
                onClick={() => setSelection(null)}
                className="btn px-2"
              >
                Clear finding
              </button>
            </>
          )}
          {handle === null && pace && (
            <span
              className={`text-sm ${pace.heavy ? "text-warning" : "text-fg-faint"}`}
            >
              {pace.text}
            </span>
          )}
          <span className="nums ml-auto text-sm text-fg-faint">
            {live
              ? `Crawling · ${live.progress.written.toLocaleString()} done · ${live.progress.queued.toLocaleString()} queued · ${live.progress.urlsPerSecond.toFixed(1)} URL/s`
              : handle
                ? basename(handle.path)
                : "Idle"}
          </span>
          <span className="nums shrink-0 text-sm text-fg-faint">
            {viewLabel}
          </span>
        </footer>
        </Panel>

        <Separator className="split split-h" />
        <Panel id="detail" defaultSize="32%" minSize="12%" className="flex min-h-0 flex-col">
        {/* Always present, even with nothing selected. The pane is part of
            the interface a new user reads at rest, not a thing that appears
            once they already know to click a row. */}
        <Detail
          id={opened}
          rules={rules}
          onClose={() => {
            setOpened(null);
            // Back where the keyboard was. Closing a pane that took focus and
            // leaving focus on nothing is how a keyboard user loses their
            // place in a list of half a million rows.
            gridFocus.current?.();
          }}
        />
        </Panel>
        </Group>
        </main>
      </Panel>

      <Separator className="split split-v" />

      <Panel id="panel" defaultSize="22%" minSize="14%" maxSize="45%" className="flex min-h-0 min-w-0 flex-col">
      {/* The overview on the right, which is where Screaming Frog puts it and
          where it belongs: the grid is the thing being read, and a panel that
          summarises it should not sit between the reader and the left edge. */}
      <Overview
        title={viewLabel}
        groups={panelGroups}
        issues={overview}
        rules={rules}
        total={pages}
        selection={selection}
        activeFilters={barKey}
        onPick={pick}
      />
      </Panel>
    </Group>
  );
}
