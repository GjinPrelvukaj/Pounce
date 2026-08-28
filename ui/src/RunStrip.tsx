import { LiveProgress } from "./LiveProgress";
import { pauseCrawl, resumeCrawl, type ProgressEvent } from "./engine";

/// The crawl in flight, above the results it is producing.
///
/// A strip rather than a screen: the point of T4.21 is that the results are
/// already there while this is ticking, so the progress must sit beside them
/// and not in front of them.
export function RunStrip({ progress }: { progress: ProgressEvent }) {
  const paused = progress.status === "paused";
  const done = progress.status !== "running" && !paused;

  return (
    // `items-center` and one row of padding. This strip used to be two stacked
    // rows of 20px figures — about 110px of the window, held for the length of
    // a crawl, above the table the crawl is filling.
    <section className="flex flex-wrap items-center justify-between gap-x-6 gap-y-2 border-b border-border bg-surface px-4 py-2">
      <LiveProgress progress={progress} />
      {/* Pause only. Stopping is already in the toolbar above and was the
          same `cancelCrawl()` call — two controls for one action, and this
          one, at "Stop and keep what is crawled", was the widest thing in the
          strip. The reassurance moved to the toolbar button's tooltip, where
          the action actually is. */}
      {!done && (
        <button
          onClick={() => void (paused ? resumeCrawl() : pauseCrawl())}
          className="btn shrink-0"
        >
          {paused ? "Resume" : "Pause"}
        </button>
      )}
    </section>
  );
}
