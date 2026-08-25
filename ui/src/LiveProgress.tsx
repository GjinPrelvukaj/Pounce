import type { ProgressEvent } from "./engine";

/// A number with its unit, sized for scanning rather than reading. Mono with
/// nums figures throughout: these change ten times a second, and digits
/// that shift width make a still screen look busy.
function Stat({
  label,
  value,
  unit,
}: {
  label: string;
  value: string;
  unit?: string;
}) {
  return (
    <div className="flex min-w-24 flex-col gap-0.5">
      <span className="text-sm text-fg-faint">{label}</span>
      <span className="nums text-lg leading-none text-fg">
        {value}
        {unit && <span className="ml-1 text-sm text-fg-muted">{unit}</span>}
      </span>
    </div>
  );
}

/// Status classes, each with an icon, a word, and the code as metadata.
///
/// `2xx` is exact and means nothing to an account manager; "Worked" is the
/// thing they came to find out. The code stays in the tooltip rather than
/// disappearing, because a specialist reads in codes and both people are
/// looking at the same crawl.
///
/// The icon is not decoration either: PRODUCT.md's binding rule is that a state
/// is never carried by colour alone, so a reader who cannot separate the hues
/// still gets a triangle and the word "Not found".
const CLASSES = [
  { label: "Informational", code: "1xx", icon: "·", className: "text-fg-muted" },
  { label: "Worked", code: "2xx", icon: "✓", className: "text-pass" },
  { label: "Redirected", code: "3xx", icon: "→", className: "text-notice" },
  { label: "Not found", code: "4xx", icon: "▲", className: "text-warning" },
  { label: "Server error", code: "5xx", icon: "●", className: "text-critical" },
] as const;

/// How a run ended, in the user's words rather than the enum's.
const STATUS_TEXT: Record<ProgressEvent["status"], string> = {
  running: "Crawling",
  paused: "Paused",
  completed: "Finished",
  cancelled: "Cancelled",
  countLimitReached: "Stopped — URL limit reached",
  timeLimitReached: "Stopped — time budget reached",
  failed: "Failed",
};

export function LiveProgress({ progress }: { progress: ProgressEvent }) {
  const seconds = progress.elapsedMs / 1000;
  const done = progress.status !== "running" && progress.status !== "paused";

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-end gap-6">
        <Stat
          label="Status"
          value={STATUS_TEXT[progress.status] ?? progress.status}
        />
        <Stat label="Pages done" value={progress.written.toLocaleString()} />
        {/* "Queued 0 waiting" was two words for one idea, one of them a
            lifecycle term. This is the number that says whether a crawl is
            nearly done or has barely started. */}
        <Stat label="Still to fetch" value={progress.queued.toLocaleString()} />
        <Stat
          label="Rate"
          value={progress.urlsPerSecond.toFixed(0)}
          unit="URL/s"
        />
        <Stat
          label="Elapsed"
          value={
            seconds < 60
              ? seconds.toFixed(1)
              : `${Math.floor(seconds / 60)}m ${Math.floor(seconds % 60)
                  .toString()
                  .padStart(2, "0")}s`
          }
          unit={seconds < 60 ? "s" : undefined}
        />
      </div>

      <div className="flex flex-wrap items-center gap-2">
        {CLASSES.map((c, i) => {
          const count = progress.byClass[i] ?? 0;
          // Absent classes are hidden rather than shown as zero: a healthy
          // crawl would otherwise carry three permanent zeroes, and the row is
          // meant to be read at a glance.
          if (count === 0 && !(i === 1 && done)) return null;
          return (
            <span
              key={c.label}
              title={`HTTP ${c.code}`}
              className={`flex items-center gap-1.5 rounded-sm border border-border bg-raised px-2 py-1 text-sm ${c.className}`}
            >
              <span aria-hidden>{c.icon}</span>
              {c.label}
              <span className="nums text-fg-muted">
                {count.toLocaleString()}
              </span>
            </span>
          );
        })}
        {progress.failed > 0 && (
          <span
            title="DNS failures, timeouts, and URLs robots.txt disallows"
            className="flex items-center gap-1.5 rounded-sm border border-border bg-raised px-2 py-1 text-sm text-critical"
          >
            <span aria-hidden>✕</span>
            Never answered
            <span className="nums text-fg-muted">
              {progress.failed.toLocaleString()}
            </span>
          </span>
        )}
      </div>
    </div>
  );
}
