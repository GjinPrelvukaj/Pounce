import { Channel, invoke } from "@tauri-apps/api/core";

/// Mirrors the shapes in `pounce-app`. Tauri serialises commands as JSON, so
/// nothing checks these definitions against the Rust ones at build time — keep
/// them together and change them together.

export type EngineInfo = {
  version: string;
  schemaVersion: number;
  rules: number;
};

export type CrawlHandle = {
  path: string;
  pages: number;
  schemaVersion: number;
};

export type RowView = {
  id: number;
  url: string;
  status: number;
  depth: number;
  size: number;
  wordCount: number;
  title: string | null;
  kind: string;
  noindex: boolean;
  /// Absent is not empty, all the way out to the grid.
  metaDescription: string | null;
  canonical: string | null;
  elapsedMs: number;
};

/// A window of the grid and the size of the result it came from. `total` is
/// what the scrollbar is drawn from; `rows` is only what is on screen.
export type Page = {
  rows: RowView[];
  total: number;
  offset: number;
  limit: number;
};

/// A rule as the interface talks about it: the sentence, not the key.
export type RuleInfo = {
  id: string;
  description: string;
  remediation: string;
  severity: string;
};

/// What the crawl contains, as opposed to what is wrong with it.
export type CrawlOverview = {
  crawled: number;
  queued: number;
  failed: number;
  byKind: [string, number][];
  byClass: [number, number, number, number, number];
  indexable: number;
  noindex: number;
};

export type IssueCount = {
  ruleId: string;
  severity: string;
  issues: number;
  urls: number;
};

export type IssueOverview = {
  byRule: IssueCount[];
  bySeverity: [string, number][];
  totalIssues: number;
  urlsWithIssues: number;
};

/// One edge of the link graph, in whichever direction it was asked for.
export type LinkRow = {
  url: string;
  anchorText: string;
  nofollow: boolean;
  /// Whether that URL is a page in this crawl.
  crawled: boolean;
};

export type DetailIssue = {
  ruleId: string;
  severity: string;
  detail: string | null;
};

/// Everything about one page. The five repeating fields arrive as parsed JSON
/// — they are stored as JSON text and the engine parses them on the way out.
export type PageDetail = {
  id: number;
  url: string;
  status: number;
  depth: number;
  size: number;
  truncated: boolean;
  contentType: string | null;
  charset: string | null;
  kind: string;
  contentTypeMismatch: boolean;
  elapsedMs: number;
  timeToHeadersMs: number;
  title: string | null;
  metaDescription: string | null;
  canonical: string | null;
  canonicalUrl: string | null;
  noindex: boolean;
  nofollow: boolean;
  noarchive: boolean;
  nosnippet: boolean;
  wordCount: number;
  redirectChain: string[] | null;
  h1: string[] | null;
  h2: string[] | null;
  hreflang: { lang: string; href: string }[] | null;
  openGraph: [string, string][] | null;
  images: { src: string; alt: string | null }[] | null;
  issues: DetailIssue[];
  /// Capped at 100 each; the counts beside them are not capped.
  inlinks: LinkRow[];
  outlinks: LinkRow[];
  inlinkCount: number;
  outlinkCount: number;
};

export type Comparison = "eq" | "ne" | "lt" | "le" | "gt" | "ge";
export type BodyKind = "html" | "pdf" | "image" | "other" | "undeclared";

export type Filter =
  | { field: "status"; cmp: Comparison; value: number }
  | { field: "depth"; cmp: Comparison; value: number }
  | { field: "wordCount"; cmp: Comparison; value: number }
  | { field: "kind"; value: BodyKind }
  | { field: "noindex"; value: boolean }
  | { field: "hasIssue"; rule: string | null }
  | { field: "urlContains"; needle: string };

export type SortColumn =
  | "url"
  | "status"
  | "depth"
  | "size"
  | "wordCount"
  | "elapsedMs"
  | "title"
  | "metaDescription";

/// The typed failures the commands return. The UI branches on `kind` rather
/// than matching on message text.
/// One tick of a running crawl. Arrives at ~10 Hz — the engine's own
/// `PROGRESS_INTERVAL` — never once per URL.
export type ProgressEvent = {
  status:
    | "running"
    | "paused"
    | "completed"
    | "cancelled"
    | "countLimitReached"
    | "timeLimitReached"
    | "failed";
  admitted: number;
  written: number;
  elapsedMs: number;
  urlsPerSecond: number;
  /// Discovered and not yet fetched — the number that says whether a crawl is
  /// nearly done or has barely started.
  queued: number;
  /// `[1xx, 2xx, 3xx, 4xx, 5xx]`.
  byClass: [number, number, number, number, number];
  /// Fetches that produced no response at all: DNS failures, timeouts, robots
  /// refusals.
  failed: number;
};

export type ApiError =
  | { kind: "noCrawlOpen" }
  | { kind: "unknownRule"; rule: string }
  | { kind: "unsupportedPair"; filter: string; sort: string }
  | { kind: "outputExists"; path: string; suggestion: string }
  | { kind: "badSeed"; input: string; message: string; suggestion: string | null }
  | { kind: "notACrawl"; path: string }
  | { kind: "missing"; path: string }
  | { kind: "export"; message: string }
  | { kind: "crawl"; message: string }
  | { kind: "store"; message: string };

export function inDesktopShell(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!inDesktopShell()) {
    return Promise.reject(
      new Error(
        "No engine here: this page is running in a browser. Use `cargo run -p pounce-app`.",
      ),
    );
  }
  return invoke<T>(command, args);
}

export const engineInfo = () => call<EngineInfo>("engine_info");
export const listRules = () => call<RuleInfo[]>("rules");
/// Where a crawl of this address would be saved unless the user says
/// otherwise. Proposed, not imposed — the form shows it and offers to change
/// it, so starting a crawl is one field and a button.
export const suggestOutput = (seed: string) =>
  call<string>("suggest_output", { seed });
export const openCrawl = (path: string) => call<CrawlHandle>("open_crawl", { path });
export const closeCrawl = () => call<void>("close_crawl");
export const currentCrawl = () => call<string | null>("current_crawl");
/// Why a file handed to the process on the command line did not open. `null`
/// in the ordinary case, including when there was no file.
export const startupError = () => call<ApiError | null>("startup_error");
export const pageDetail = (id: number) =>
  call<PageDetail | null>("page_detail", { id });
export const crawlOverview = () => call<CrawlOverview>("crawl_overview");
export const issueOverview = () => call<IssueOverview>("issue_overview");

export const queryRows = (args: {
  filters: Filter[];
  sort: SortColumn;
  direction: "asc" | "desc";
  offset: number;
  limit: number;
}) => call<Page>("query_rows", args);

/// Streams the current view to a file and resolves with the number of rows
/// written. The dataset never crosses this boundary — the engine writes it to
/// disk directly, which is what makes a 500,000-row export a file rather than a
/// gigabyte of IPC.
export const exportRows = (args: {
  path: string;
  filters: Filter[];
  sort: SortColumn;
  direction: "asc" | "desc";
}) => call<number>("export_rows", args);

export const supportedSorts = (filters: Filter[]) =>
  call<SortColumn[]>("supported_sorts", { filters });

/// What the new-crawl screen collects. Everything but the seed and the output
/// is optional — an unset limit means no limit, and unset politeness means the
/// engine's own defaults.
export type CrawlSettings = {
  seed: string;
  output: string;
  images: boolean;
  maxDepth: number | null;
  maxUrls: number | null;
  maxDurationSecs: number | null;
  perHostConcurrency: number | null;
  delayMs: number | null;
};

/// The politeness ceiling the engine enforces. Mirrored here so the form can
/// say so before the command refuses.
export const MAX_PER_HOST_CONCURRENCY = 16;

/// Starts a crawl. Resolves with the finished file, which the engine leaves
/// open — the grid can query it without a second round trip.
export function startCrawl(
  settings: CrawlSettings,
  onProgress: (p: ProgressEvent) => void,
): Promise<CrawlHandle> {
  const channel = new Channel<ProgressEvent>();
  channel.onmessage = onProgress;
  return call<CrawlHandle>("start_crawl", { settings, onProgress: channel });
}

/// Pause, resume and cancel the crawl in flight. Each resolves to whether it
/// changed anything — `false` means there was nothing running to act on,
/// which is not an error: the last batch may land while the button is still
/// on screen.
export const pauseCrawl = () => call<boolean>("pause_crawl");
export const resumeCrawl = () => call<boolean>("resume_crawl");
export const cancelCrawl = () => call<boolean>("cancel_crawl");
