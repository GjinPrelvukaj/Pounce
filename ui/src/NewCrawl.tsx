import { useState } from "react";
import { LiveProgress } from "./LiveProgress";
import {
  MAX_PER_HOST_CONCURRENCY,
  cancelCrawl,
  pauseCrawl,
  resumeCrawl,
  startCrawl,
  type CrawlHandle,
  type CrawlSettings,
  type ProgressEvent,
} from "./engine";

/// A number field that means "unset" when empty.
///
/// The distinction is the whole point: an empty depth box is *no depth limit*,
/// not depth zero. Storing the raw string keeps a half-typed "1" from becoming
/// a limit of 1 while the user is still reaching for the 0.
function NumberField({
  label,
  value,
  onChange,
  placeholder,
  suffix,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  placeholder: string;
  suffix?: string;
}) {
  return (
    <label className="flex flex-col gap-1">
      <span className="text-xs text-fg-muted">{label}</span>
      <span className="flex items-baseline gap-1">
        <input
          value={value}
          onChange={(e) => onChange(e.target.value.replace(/[^0-9]/g, ""))}
          placeholder={placeholder}
          inputMode="numeric"
          className="tabular w-24 rounded-sm border border-border bg-raised px-2 py-1 text-xs text-fg outline-none placeholder:text-fg-faint focus:border-accent-line"
        />
        {suffix && <span className="text-xs text-fg-faint">{suffix}</span>}
      </span>
    </label>
  );
}

const orNull = (v: string) => (v.trim() === "" ? null : Number(v));

export function NewCrawl({ onDone }: { onDone: (h: CrawlHandle) => void }) {
  const [seed, setSeed] = useState("");
  const [output, setOutput] = useState("");
  const [images, setImages] = useState(false);
  const [maxDepth, setMaxDepth] = useState("");
  const [maxUrls, setMaxUrls] = useState("");
  const [maxDuration, setMaxDuration] = useState("");
  const [concurrency, setConcurrency] = useState("");
  const [delay, setDelay] = useState("");

  const [progress, setProgress] = useState<ProgressEvent | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);

  const settings = (): CrawlSettings => ({
    seed: seed.trim(),
    output: output.trim(),
    images,
    maxDepth: orNull(maxDepth),
    maxUrls: orNull(maxUrls),
    maxDurationSecs: orNull(maxDuration),
    perHostConcurrency: orNull(concurrency),
    delayMs: orNull(delay),
  });

  async function start() {
    setRunning(true);
    setError(null);
    setProgress(null);
    try {
      const handle = await startCrawl(settings(), setProgress);
      onDone(handle);
    } catch (e) {
      const api = e as { kind?: string; message?: string };
      setError(api?.message ?? String(e));
    } finally {
      setRunning(false);
    }
  }

  const paused = progress?.status === "paused";
  const ready = seed.trim() !== "" && output.trim() !== "" && !running;

  return (
    <section className="flex flex-col gap-4 border-b border-border bg-surface px-4 py-3">
      <div className="flex flex-wrap items-end gap-3">
        <label className="flex flex-col gap-1">
          <span className="text-xs text-fg-muted">Seed URL</span>
          <input
            value={seed}
            onChange={(e) => setSeed(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && ready && void start()}
            placeholder="https://example.com/"
            spellCheck={false}
            className="tabular w-80 rounded-sm border border-border bg-raised px-2 py-1 text-xs text-fg outline-none placeholder:text-fg-faint focus:border-accent-line"
          />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-xs text-fg-muted">Save to</span>
          <input
            value={output}
            onChange={(e) => setOutput(e.target.value)}
            placeholder="~/crawls/example.pounce"
            spellCheck={false}
            className="tabular w-64 rounded-sm border border-border bg-raised px-2 py-1 text-xs text-fg outline-none placeholder:text-fg-faint focus:border-accent-line"
          />
        </label>
        <button
          onClick={() => void start()}
          disabled={!ready}
          className="rounded-sm bg-accent px-3 py-1.5 text-xs text-on-accent transition-colors duration-150 ease-state disabled:opacity-50"
        >
          {running ? "Crawling…" : "Start crawl"}
        </button>
        {running && (
          <>
            <button
              onClick={() => void (paused ? resumeCrawl() : pauseCrawl())}
              className="rounded-sm border border-border px-3 py-1.5 text-xs text-fg-muted transition-colors duration-150 ease-state hover:text-fg"
            >
              {paused ? "Resume" : "Pause"}
            </button>
            <button
              onClick={() => void cancelCrawl()}
              className="rounded-sm border border-border px-3 py-1.5 text-xs text-fg-muted transition-colors duration-150 ease-state hover:text-critical"
            >
              Cancel
            </button>
          </>
        )}
      </div>

      <div className="flex flex-wrap items-end gap-4">
        <fieldset className="flex flex-wrap items-end gap-3">
          <legend className="mb-1 text-xs text-fg-faint">
            Limits — empty means no limit
          </legend>
          <NumberField
            label="Max depth"
            value={maxDepth}
            onChange={setMaxDepth}
            placeholder="∞"
          />
          <NumberField
            label="Max URLs"
            value={maxUrls}
            onChange={setMaxUrls}
            placeholder="∞"
          />
          <NumberField
            label="Time budget"
            value={maxDuration}
            onChange={setMaxDuration}
            placeholder="∞"
            suffix="s"
          />
        </fieldset>

        <fieldset className="flex flex-wrap items-end gap-3">
          <legend className="mb-1 text-xs text-fg-faint">Politeness</legend>
          <NumberField
            label="Per-host requests"
            value={concurrency}
            onChange={setConcurrency}
            placeholder="4"
            suffix={`max ${MAX_PER_HOST_CONCURRENCY}`}
          />
          <NumberField
            label="Delay per request"
            value={delay}
            onChange={setDelay}
            placeholder="0"
            suffix="ms"
          />
          <label className="flex items-center gap-1.5 pb-1.5">
            <input
              type="checkbox"
              checked={images}
              onChange={(e) => setImages(e.target.checked)}
              className="accent-accent"
            />
            <span className="text-xs text-fg-muted">Check images</span>
          </label>
        </fieldset>
      </div>

      {/* Stated rather than offered. robots.txt is honoured with no way to turn
          it off, and a crawler that gets its user blocked is a liability — so
          this is a default, not a setting. */}
      <p className="text-xs text-fg-faint">
        robots.txt is always honoured, <code>Retry-After</code> is always
        respected, and requests carry an identifying user agent. Raising
        per-host requests or checking images makes a crawl heavier on the site —
        both are opt-in for that reason.
      </p>

      {error && <p className="tabular text-xs text-critical">{error}</p>}

      {progress && <LiveProgress progress={progress} />}
    </section>
  );
}
