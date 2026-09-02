import type { ProgressEvent } from "./engine";

/// A number and what it counts, on one line.
///
/// Was a stacked pair — label above a 20px value — which read well and cost
/// two rows of a strip that sits above the results for the whole crawl. Value
/// first because it is what you are looking for; the label after it, small and
/// quiet, because you already know which number is which after the first
/// glance. The figures are `nums`, so a digit changing ten times a second does
/// not shift the row.
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
    <div className="flex items-baseline gap-2">
      <span className="nums text-md text-fg">{value}</span>
      {unit && <span className="text-xs text-fg-muted">{unit}</span>}
      <span className="text-xs text-fg-faint">{label}</span>
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
  countLimitReached: "Stopped: URL limit reached",
  timeLimitReached: "Stopped: time budget reached",
  failed: "Failed",
};

export function LiveProgress({ progress }: { progress: ProgressEvent }) {
  const seconds = progress.elapsedMs / 1000;
  const done = progress.status !== "running" && progress.status !== "paused";
  const running = progress.status === "running";

  return (
    // One row, wrapping rather than shrinking. The strip sits above the
    // results for the length of a crawl, so every pixel of it is a pixel of
    // the table someone is watching — but it is read at a glance, and a row
    // squeezed to fit is read at no glance at all.
    <div className="flex flex-wrap items-baseline gap-x-4 gap-y-2">
      <span
        className={`text-md font-medium ${running ? "text-accent-fg" : "text-fg"}`}
      >
        {STATUS_TEXT[progress.status] ?? progress.status}
      </span>
      <Stat label="done" value={progress.written.toLocaleString()} />
      {/* "Queued 0 waiting" was two words for one idea, one of them a
          lifecycle term. This is the number that says whether a crawl is
          nearly done or has barely started. */}
      <Stat label="to fetch" value={progress.queued.toLocaleString()} />
      <Stat
        label=""
        value={progress.urlsPerSecond.toFixed(0)}
        unit="URL/s"
      />
      <Stat
        label="elapsed"
        value={
          seconds < 60
            ? seconds.toFixed(1)
            : `${Math.floor(seconds / 60)}m ${Math.floor(seconds % 60)
                .toString()
                .padStart(2, "0")}s`
        }
        unit={seconds < 60 ? "s" : undefined}
      />

      {/* The chips lose their border and ground. An icon, a word and a count
          in the severity's own hue is the whole message, and a bordered pill
          around each one was a second row's worth of height spent on
          decoration. Icon and word both stay: never colour alone. */}
      <span className="flex flex-wrap items-baseline gap-x-4 gap-y-1">
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
              className={`flex items-baseline gap-2 text-sm ${c.className}`}
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
            className="flex items-baseline gap-2 text-sm text-critical"
          >
            <span aria-hidden>✕</span>
            Never answered
            <span className="nums text-fg-muted">
              {progress.failed.toLocaleString()}
            </span>
          </span>
        )}
      </span>
    </div>
  );
}
