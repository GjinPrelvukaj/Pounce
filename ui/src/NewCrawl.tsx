import { useState } from "react";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { MAX_PER_HOST_CONCURRENCY, type CrawlSettings } from "./engine";

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
      <span className="text-sm text-fg-muted">{label}</span>
      <span className="flex items-baseline gap-1">
        <input
          value={value}
          onChange={(e) => onChange(e.target.value.replace(/[^0-9]/g, ""))}
          placeholder={placeholder}
          inputMode="numeric"
          className="field tabular w-24 placeholder:text-fg-faint"
        />
        {suffix && <span className="text-xs text-fg-faint">{suffix}</span>}
      </span>
    </label>
  );
}

const orNull = (v: string) => (v.trim() === "" ? null : Number(v));

/// The setup screen: what to crawl, how far, and how gently.
///
/// A form and nothing else. It used to own the crawl as well, which is why the
/// results could not appear until it was done with them — the run now belongs
/// to the shell, and this hands it a settled `CrawlSettings` and steps aside.
export function NewCrawl({
  busy,
  error,
  onStart,
  onCancel,
}: {
  busy: boolean;
  error: string | null;
  onStart: (settings: CrawlSettings) => void;
  onCancel: () => void;
}) {
  const [seed, setSeed] = useState("");
  const [output, setOutput] = useState("");
  const [images, setImages] = useState(false);
  const [maxDepth, setMaxDepth] = useState("");
  const [maxUrls, setMaxUrls] = useState("");
  const [maxDuration, setMaxDuration] = useState("");
  const [concurrency, setConcurrency] = useState("");
  const [delay, setDelay] = useState("");

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

  /// The save dialog names the file; the engine refuses to overwrite one that
  /// exists, so this is where a user picks a fresh name rather than discovering
  /// the refusal after configuring a crawl.
  async function chooseOutput() {
    const chosen = await saveDialog({
      defaultPath: "crawl.pounce",
      filters: [{ name: "Pounce crawl", extensions: ["pounce"] }],
    });
    if (typeof chosen === "string") setOutput(chosen);
  }

  const ready = seed.trim() !== "" && output.trim() !== "" && !busy;
  const start = () => ready && onStart(settings());

  return (
    <main className="flex min-h-0 flex-1 flex-col gap-4 overflow-auto p-6">
      <h2 className="text-lg font-semibold">New crawl</h2>
      <div className="flex flex-wrap items-end gap-3">
        <label className="flex flex-col gap-1">
          <span className="text-sm text-fg-muted">Seed URL</span>
          <input
            value={seed}
            onChange={(e) => setSeed(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && start()}
            placeholder="https://example.com/"
            spellCheck={false}
            className="field tabular w-80 placeholder:text-fg-faint"
          />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-sm text-fg-muted">Save to</span>
          <span className="flex items-center gap-1">
            <input
              value={output}
              onChange={(e) => setOutput(e.target.value)}
              placeholder="~/crawls/example.pounce"
              spellCheck={false}
              className="field tabular w-64 placeholder:text-fg-faint"
            />
            <button
              onClick={() => void chooseOutput()}
              className="btn"
            >
              Choose…
            </button>
          </span>
        </label>
        <button
          onClick={start}
          disabled={!ready}
          className="btn btn-primary"
        >
          {busy ? "Starting…" : "Start crawl"}
        </button>
        <button onClick={onCancel} className="btn">
          Cancel
        </button>
      </div>

      <div className="flex flex-wrap items-end gap-4">
        <fieldset className="flex flex-wrap items-end gap-3">
          <legend className="mb-1 text-sm text-fg-faint">
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
          <legend className="mb-1 text-sm text-fg-faint">Politeness</legend>
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
              className="focusable accent-accent"
            />
            <span className="text-sm text-fg-muted">Check images</span>
          </label>
        </fieldset>
      </div>

      {/* Stated rather than offered. robots.txt is honoured with no way to turn
          it off, and a crawler that gets its user blocked is a liability — so
          this is a default, not a setting. */}
      <p className="text-sm text-fg-faint">
        robots.txt is always honoured, <code>Retry-After</code> is always
        respected, and requests carry an identifying user agent. Raising
        per-host requests or checking images makes a crawl heavier on the site —
        both are opt-in for that reason.
      </p>

      {error && <p className="text-md text-critical">{error}</p>}
    </main>
  );
}
