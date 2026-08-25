import { LiveProgress } from "./LiveProgress";
import { cancelCrawl, pauseCrawl, resumeCrawl, type ProgressEvent } from "./engine";

/// The crawl in flight, above the results it is producing.
///
/// A strip rather than a screen: the point of T4.21 is that the results are
/// already there while this is ticking, so the progress must sit beside them
/// and not in front of them.
export function RunStrip({ progress }: { progress: ProgressEvent }) {
  const paused = progress.status === "paused";
  const done = progress.status !== "running" && !paused;

  return (
    <section className="flex flex-wrap items-end justify-between gap-4 border-b border-border bg-surface px-4 py-2.5">
      <LiveProgress progress={progress} />
      {!done && (
        <div className="flex gap-2">
          <button
            onClick={() => void (paused ? resumeCrawl() : pauseCrawl())}
            className="btn"
          >
            {paused ? "Resume" : "Pause"}
          </button>
          <button
            onClick={() => void cancelCrawl()}
            className="btn hover:!text-critical"
          >
            Stop and keep what is crawled
          </button>
        </div>
      )}
    </section>
  );
}
