import { useEffect, useRef, useState } from "react";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import {
  MAX_PER_HOST_CONCURRENCY,
  suggestOutput,
  type CrawlSettings,
} from "./engine";

/// The three paces, and the two numbers each one means.
///
/// `Gentle` is one request a second on purpose: 60 a minute is the limit small
/// sites and shared hosts most commonly enforce.
export const PACES = [
  { id: "gentle", label: "Gentle", concurrency: 1, delay: 1000 },
  { id: "normal", label: "Normal", concurrency: 4, delay: 0 },
  { id: "fast", label: "Fast", concurrency: 8, delay: 0 },
] as const;

/// What the two politeness numbers do to the site on the other end.
///
/// Arithmetic when there is a delay, because `concurrency / delay` is a ceiling
/// the scheduler enforces. Honestly unknowable when there is not, because the
/// crawl then runs at whatever speed the server answers, and saying so is both
/// the true answer and the warning.
export function politeness(concurrency: number, delayMs: number) {
  if (delayMs <= 0) {
    return {
      text: `${concurrency} request${concurrency === 1 ? "" : "s"} at a time and no waiting: as fast as the site answers. Many small sites allow about 60 requests a minute.`,
      heavy: true,
    };
  }
  const rate = concurrency / (delayMs / 1000);
  return {
    text: `At most ${rate < 1 ? rate.toFixed(2) : Math.round(rate)} request${rate === 1 ? "" : "s"} a second: ${
      rate <= 1
        ? "gentle enough for a site that limits its visitors"
        : rate <= 5
          ? "fine for most sites"
          : "heavy for a site you do not control"
    }.`,
    heavy: rate > 5,
  };
}

/// Adds the scheme a browser would have added. People type `example.com`, and
/// doing what they meant and showing them beats refusing and negotiating.
export function normalise(input: string): string {
  const trimmed = input.trim();
  if (trimmed === "" || trimmed.includes("://")) return trimmed;
  return `https://${trimmed}`;
}

const orNull = (v: string) => (v.trim() === "" ? null : Number(v));

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
      <span className="flex items-baseline gap-1.5">
        <input
          value={value}
          onChange={(e) => onChange(e.target.value.replace(/[^0-9]/g, ""))}
          placeholder={placeholder}
          inputMode="numeric"
          className="field nums w-20 placeholder:text-fg-faint"
        />
        {suffix && <span className="text-sm text-fg-faint">{suffix}</span>}
      </span>
    </label>
  );
}

/// The crawl toolbar: an address, a pace, and Start.
///
/// It lives in the header and never leaves. Screaming Frog's URL bar is always
/// on screen, so re-crawling is one field away and you never navigate back to a
/// setup screen to do it. Ours used to be a separate screen you entered and
/// exited, which is one of the two things that made this app feel harder than
/// it is.
///
/// Everything else (limits, exact concurrency, output path, images) is behind
/// Options, because those have defaults that are right for most crawls.
export function CrawlBar({
  running,
  busy,
  pending,
  onStart,
  onStop,
  onClear,
  canClear,
  onPaceText,
}: {
  running: boolean;
  busy: boolean;
  /// A correction the user accepted from a typed error ("Save as site-2.pounce
  /// instead"), applied to these fields rather than retyped.
  pending: { label: string; apply: (s: CrawlSettings) => CrawlSettings } | null;
  onStart: (settings: CrawlSettings) => void;
  onStop: () => void;
  onClear: () => void;
  canClear: boolean;
  /// Publishes the current pace sentence so the status bar can carry it
  /// without this toolbar growing a second row.
  onPaceText: (text: string, heavy: boolean) => void;
}) {
  const [seed, setSeed] = useState("");
  const [output, setOutput] = useState("");
  const [pace, setPace] = useState<string>("normal");
  const [concurrency, setConcurrency] = useState("");
  const [delay, setDelay] = useState("");
  const [images, setImages] = useState(false);
  const [maxDepth, setMaxDepth] = useState("");
  const [maxUrls, setMaxUrls] = useState("");
  const [maxMinutes, setMaxMinutes] = useState("");
  const chosen = useRef(false);
  const options = useRef<HTMLDialogElement>(null);

  // A pace preset sets the two numbers; typing a number by hand makes the pace
  // "Custom" rather than silently disagreeing with the selector.
  const preset = PACES.find((p) => p.id === pace);
  const activeConcurrency = orNull(concurrency) ?? preset?.concurrency ?? 4;
  const activeDelay = orNull(delay) ?? preset?.delay ?? 0;
  const pace_ = politeness(activeConcurrency, activeDelay);

  useEffect(() => {
    onPaceText(pace_.text, pace_.heavy);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pace_.text, pace_.heavy]);

  useEffect(() => {
    if (!pending) return;
    const next = pending.apply(settings());
    setSeed(next.seed);
    if (next.output !== output) {
      chosen.current = true;
      setOutput(next.output);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pending]);

  // The proposed file name follows the address until someone overrides it.
  useEffect(() => {
    const url = normalise(seed);
    if (chosen.current || url === "") return;
    let live = true;
    const id = setTimeout(() => {
      suggestOutput(url)
        .then((path) => live && !chosen.current && setOutput(path))
        .catch(() => {});
    }, 250);
    return () => {
      live = false;
      clearTimeout(id);
    };
  }, [seed]);

  const settings = (): CrawlSettings => ({
    seed: normalise(seed),
    output: output.trim(),
    images,
    maxDepth: orNull(maxDepth),
    maxUrls: orNull(maxUrls),
    maxDurationSecs: maxMinutes.trim() === "" ? null : Number(maxMinutes) * 60,
    perHostConcurrency: activeConcurrency,
    delayMs: activeDelay,
  });

  const ready = normalise(seed) !== "" && output.trim() !== "" && !busy && !running;
  const start = () => ready && onStart(settings());

  async function chooseOutput() {
    const picked = await saveDialog({
      defaultPath: output || "crawl.pounce",
      filters: [{ name: "Pounce crawl", extensions: ["pounce"] }],
    });
    if (typeof picked === "string") {
      chosen.current = true;
      setOutput(picked);
    }
  }

  return (
    <>
      <input
        value={seed}
        onChange={(e) => setSeed(e.target.value)}
        onBlur={() => setSeed(normalise(seed))}
        onKeyDown={(e) => {
          if (e.key !== "Enter") return;
          setSeed(normalise(seed));
          start();
        }}
        placeholder="Enter a website address to crawl"
        aria-label="Website address"
        spellCheck={false}
        disabled={running}
        className="field nums min-w-0 flex-1 placeholder:text-fg-faint"
      />

      <label className="shrink-0">
        <span className="sr-only">Pace</span>
        <select
          value={preset ? pace : "custom"}
          onChange={(e) => {
            setPace(e.target.value);
            setConcurrency("");
            setDelay("");
          }}
          aria-label="Crawl pace"
          title={pace_.text}
          disabled={running}
          className={`field text-sm ${pace_.heavy ? "border-warning text-warning" : "text-fg-muted"}`}
        >
          {PACES.map((p) => (
            <option key={p.id} value={p.id}>
              {p.label}
            </option>
          ))}
          {!preset && <option value="custom">Custom</option>}
        </select>
      </label>

      {running ? (
        <button
          onClick={onStop}
          title="Ends the crawl and keeps every page already saved"
          className="btn shrink-0 hover:!text-critical"
        >
          Stop
        </button>
      ) : (
        <button
          onClick={start}
          disabled={!ready}
          title={ready ? undefined : "Enter a website address first"}
          className="btn btn-primary shrink-0"
        >
          {busy ? "Starting…" : "Start"}
        </button>
      )}

      <button
        onClick={onClear}
        disabled={!canClear || running}
        className="btn shrink-0"
      >
        Clear
      </button>

      <button
        onClick={() => options.current?.showModal()}
        disabled={running}
        className="btn shrink-0"
      >
        Options…
      </button>

      <dialog
        ref={options}
        onClick={(e) => e.target === options.current && options.current?.close()}
        className="raised-panel m-auto rounded-lg border border-border bg-surface p-0 text-fg"
      >
        <div className="flex w-[32rem] max-w-[92vw] flex-col gap-5 p-5">
          <h2 className="text-md font-medium">Crawl options</h2>

          <fieldset className="flex flex-col gap-2">
            <legend className="mb-1 text-xs font-semibold tracking-[0.07em] text-fg-faint uppercase">
              How fast
            </legend>
            <div className="flex flex-wrap items-end gap-3">
              <NumberField
                label="Pages at a time"
                value={concurrency}
                onChange={setConcurrency}
                placeholder={String(preset?.concurrency ?? 4)}
                suffix={`up to ${MAX_PER_HOST_CONCURRENCY}`}
              />
              <NumberField
                label="Wait between requests"
                value={delay}
                onChange={setDelay}
                placeholder={String(preset?.delay ?? 0)}
                suffix="ms"
              />
            </div>
            <p className={`text-sm ${pace_.heavy ? "text-warning" : "text-fg-muted"}`}>
              {pace_.text}
            </p>
          </fieldset>

          <fieldset className="flex flex-wrap items-end gap-4">
            <legend className="mb-1 text-xs font-semibold tracking-[0.07em] text-fg-faint uppercase">
              Stop early (leave blank to crawl the whole site)
            </legend>
            <NumberField
              label="After this many pages"
              value={maxUrls}
              onChange={setMaxUrls}
              placeholder="∞"
            />
            <NumberField
              label="After this long"
              value={maxMinutes}
              onChange={setMaxMinutes}
              placeholder="∞"
              suffix="minutes"
            />
            <NumberField
              label="Beyond this many clicks from home"
              value={maxDepth}
              onChange={setMaxDepth}
              placeholder="∞"
            />
          </fieldset>

          <label className="flex items-start gap-2">
            <input
              type="checkbox"
              checked={images}
              onChange={(e) => setImages(e.target.checked)}
              className="focusable mt-1 accent-accent"
            />
            <span className="flex flex-col">
              <span className="text-sm text-fg">Also check images</span>
              <span className="text-xs text-fg-faint">
                Finds broken and oversized images. Slower, and it sends requests
                to wherever the images are hosted.
              </span>
            </span>
          </label>

          <label className="flex flex-col gap-1">
            <span className="text-sm text-fg-muted">Save the crawl as</span>
            <span className="flex items-center gap-2">
              <input
                value={output}
                onChange={(e) => {
                  chosen.current = true;
                  setOutput(e.target.value);
                }}
                placeholder="Proposed from the address"
                spellCheck={false}
                className="field nums min-w-0 flex-1"
              />
              <button onClick={() => void chooseOutput()} className="btn">
                Change…
              </button>
            </span>
          </label>

          <p className="text-sm text-fg-faint">
            Pounce always obeys robots.txt, always waits when a site asks it to,
            and identifies itself in every request.
          </p>

          <button
            onClick={() => options.current?.close()}
            className="btn btn-primary self-end"
          >
            Done
          </button>
        </div>
      </dialog>
    </>
  );
}
