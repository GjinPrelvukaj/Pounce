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
};

/// A window of the grid and the size of the result it came from. `total` is
/// what the scrollbar is drawn from; `rows` is only what is on screen.
export type Page = {
  rows: RowView[];
  total: number;
  offset: number;
  limit: number;
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
  | "title";

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
};

export type ApiError =
  | { kind: "noCrawlOpen" }
  | { kind: "unknownRule"; rule: string }
  | { kind: "unsupportedPair"; filter: string; sort: string }
  | { kind: "crawl"; message: string }
  | { kind: "store"; message: string };

export function inDesktopShell(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!inDesktopShell()) {
    return Promise.reject(
      new Error(
        "no engine here — this page is running in a browser. Use `cargo run -p pounce-app`.",
      ),
    );
  }
  return invoke<T>(command, args);
}

export const engineInfo = () => call<EngineInfo>("engine_info");
export const openCrawl = (path: string) => call<CrawlHandle>("open_crawl", { path });
export const closeCrawl = () => call<void>("close_crawl");
export const currentCrawl = () => call<string | null>("current_crawl");
export const issueOverview = () => call<IssueOverview>("issue_overview");

export const queryRows = (args: {
  filters: Filter[];
  sort: SortColumn;
  direction: "asc" | "desc";
  offset: number;
  limit: number;
}) => call<Page>("query_rows", args);

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
