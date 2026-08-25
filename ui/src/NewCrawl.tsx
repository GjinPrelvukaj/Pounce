import { useEffect, useRef, useState } from "react";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import type { Failure } from "./App";
import {
  MAX_PER_HOST_CONCURRENCY,
  suggestOutput,
  type CrawlSettings,
} from "./engine";

/// A number field that means "unset" when empty.
///
/// The distinction is the whole point: an empty box is *no limit*, not zero.
/// Storing the raw string keeps a half-typed "1" from becoming a limit of 1
/// while the user is still reaching for the 0.
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

const orNull = (v: string) => (v.trim() === "" ? null : Number(v));

/// The engine's own default, restated so the form can describe the crawl a user
/// is about to run rather than the fields they have filled in.
const DEFAULT_CONCURRENCY = 4;

/// Presets, because the two politeness numbers are the ones that decide whether
/// a crawl is welcome or gets its user blocked, and neither of them says so.
///
/// `Gentle` is 1 request a second on purpose: 60 a minute is the limit small
/// sites and shared hosts most commonly enforce, and it is what the owner's own
/// site allows. A default crawl hit it at 7 URL/s.
const PRESETS = [
  { id: "gentle", label: "Gentle", concurrency: "1", delay: "1000" },
  { id: "normal", label: "Normal", concurrency: "4", delay: "0" },
  { id: "fast", label: "Fast", concurrency: "8", delay: "0" },
];

/// What these two numbers do to the site on the other end, in a sentence.
///
/// The rate is arithmetic when there is a delay — `concurrency / delay` is a
/// ceiling the scheduler enforces — and honestly unknowable when there is not,
/// because with no delay the crawl runs at whatever speed the server answers.
/// Saying "as fast as the server answers" is the true answer and the warning.
function politeness(concurrency: number, delayMs: number) {
  if (delayMs <= 0) {
    return {
      text: `${concurrency} request${concurrency === 1 ? "" : "s"} at a time and no waiting: as fast as the site answers. On a quick site that is tens of requests a second.`,
      heavy: true,
    };
  }
  const rate = concurrency / (delayMs / 1000);
  return {
    text: `At most ${rate < 1 ? rate.toFixed(2) : Math.round(rate)} request${rate === 1 ? "" : "s"} a second — ${
      rate <= 1
        ? "gentle enough for a site that limits its visitors"
        : rate <= 5
          ? "fine for most sites"
          : "heavy for a site you do not control"
    }.`,
    heavy: rate > 5,
  };
}

/// Adds the scheme a browser would have added.
///
/// People type `example.com`. Refusing that and offering to fix it afterwards
/// is a worse conversation than doing what they meant and showing them — the
/// field is corrected in place, so what will be crawled is on screen before
/// anything is.
function normalise(input: string): string {
  const trimmed = input.trim();
  if (trimmed === "" || trimmed.includes("://")) return trimmed;
  return `https://${trimmed}`;
}

/// The setup screen: what to crawl, and — folded away — how.
///
/// One field and a button. Every other control here has a default that is right
/// for most crawls, and a form that presents eight of them with equal weight is
/// a form that says none of them can be trusted. The file name is proposed from
/// the address rather than demanded, because "where shall I save it" is not a
/// question anyone has an opinion about until afterwards.
export function NewCrawl({
  busy,
  error,
  pending,
  onFix,
  onStart,
  onCancel,
}: {
  busy: boolean;
  error: Failure | null;
  /// A correction the user accepted, to apply to these fields.
  pending: Failure["fix"] | null;
  onFix: (fix: Failure["fix"]) => void;
  onStart: (settings: CrawlSettings) => void;
  onCancel: () => void;
}) {
  const [seed, setSeed] = useState("");
  const [output, setOutput] = useState("");
  const [images, setImages] = useState(false);
  const [maxDepth, setMaxDepth] = useState("");
  const [maxUrls, setMaxUrls] = useState("");
  const [maxMinutes, setMaxMinutes] = useState("");
  const [concurrency, setConcurrency] = useState("");
  const [delay, setDelay] = useState("");
  // Once someone picks a file the app stops proposing one. Overwriting a
  // deliberate choice with a guess is worse than never guessing.
  const chosen = useRef(false);

  const settings = (): CrawlSettings => ({
    seed: normalise(seed),
    output: output.trim(),
    images,
    maxDepth: orNull(maxDepth),
    maxUrls: orNull(maxUrls),
    // Minutes on screen, seconds on the wire: nobody budgets a crawl in
    // seconds, and the engine does not take minutes.
    maxDurationSecs: maxMinutes.trim() === "" ? null : Number(maxMinutes) * 60,
    perHostConcurrency: orNull(concurrency),
    delayMs: orNull(delay),
  });

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

  // A fix arrives as a function over settings, so the form applies it to its
  // own fields rather than each error knowing which input it belongs to.
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

  const pace = politeness(
    orNull(concurrency) ?? DEFAULT_CONCURRENCY,
    orNull(delay) ?? 0,
  );
  const ready = normalise(seed) !== "" && output.trim() !== "" && !busy;
  const start = () => ready && onStart(settings());

  return (
    <main className="flex min-h-0 flex-1 flex-col items-center overflow-auto p-8">
      <div className="flex w-full max-w-2xl flex-col gap-5">
        <div className="flex flex-col gap-1">
          <h2 className="text-md font-semibold">New crawl</h2>
          <p className="text-sm text-fg-muted">
            Type a website address. Everything else has a default that works.
          </p>
        </div>

        <div className="flex flex-col gap-2">
          <label className="flex flex-col gap-1">
            <span className="text-sm text-fg-muted">Website address</span>
            <span className="flex flex-wrap items-center gap-2">
              <input
                value={seed}
                onChange={(e) => setSeed(e.target.value)}
                onBlur={() => setSeed(normalise(seed))}
                onKeyDown={(e) => {
                  if (e.key !== "Enter") return;
                  setSeed(normalise(seed));
                  start();
                }}
                placeholder="example.com"
                spellCheck={false}
                autoFocus
                className="field nums min-w-0 flex-1 placeholder:text-fg-faint"
              />
              <button
                onClick={start}
                disabled={!ready}
                className="btn btn-primary shrink-0 px-4 py-2"
              >
                {busy ? "Starting…" : "Start crawl"}
              </button>
              <button onClick={onCancel} className="btn shrink-0">
                Cancel
              </button>
            </span>
          </label>

          {/* Proposed, and visible. A file that appears somewhere the user was
              never told about is a file they go looking for later. */}
          <p className="text-sm text-fg-faint">
            {output.trim() === "" ? (
              "Saved as a .pounce file you can reopen."
            ) : (
              <>
                Saves as{" "}
                <span className="nums text-fg-muted">{output}</span>{" "}
                <button
                  onClick={() => void chooseOutput()}
                  className="focusable rounded-sm text-accent-fg hover:underline"
                >
                  Change…
                </button>
              </>
            )}
          </p>
        </div>

        {/* Outside the fold on purpose. This is the sentence that would have
            told the owner his default crawl was about to run at seven requests
            a second against a site that allows one, and a warning behind a
            disclosure is not a warning. */}
        <p className={`text-sm ${pace.heavy ? "text-warning" : "text-fg-muted"}`}>
          {pace.text}
          {pace.heavy && (
            <>
              {" "}
              Many small sites allow about 60 requests a minute — the Gentle
              setting stays under that.
            </>
          )}
        </p>

        {error && (
          <div className="flex flex-wrap items-center gap-3">
            <p className="text-sm text-critical">{error.message}</p>
            {error.fix && (
              <button onClick={() => onFix(error.fix)} className="btn">
                {error.fix.label}
              </button>
            )}
          </div>
        )}

        {/* Folded, because the defaults are right for most crawls and a screen
            that opens with eight controls says they are not. `<details>` rather
            than a state flag: it is a disclosure widget, and the browser has
            one. */}
        <details className="rounded-md border border-border bg-surface">
          <summary className="focusable cursor-default rounded-md px-3 py-2 text-sm text-fg-muted select-none hover:text-fg">
            More options — how fast, and how much
          </summary>

          <div className="flex flex-col gap-4 border-t border-border p-3">
            <fieldset className="flex flex-col gap-2">
              <legend className="mb-1 text-xs font-semibold tracking-[0.07em] text-fg-faint uppercase">
                How fast
              </legend>
              <div className="flex flex-wrap items-end gap-3">
                <div className="flex flex-col gap-1">
                  <span className="text-sm text-fg-muted">Pace</span>
                  <div className="flex gap-1">
                    {PRESETS.map((p) => (
                      <button
                        key={p.id}
                        onClick={() => {
                          setConcurrency(p.concurrency);
                          setDelay(p.delay);
                        }}
                        aria-pressed={
                          concurrency === p.concurrency && delay === p.delay
                        }
                        className="btn"
                      >
                        {p.label}
                      </button>
                    ))}
                  </div>
                </div>
                <NumberField
                  label="Pages at a time"
                  value={concurrency}
                  onChange={setConcurrency}
                  placeholder="4"
                  suffix={`up to ${MAX_PER_HOST_CONCURRENCY}`}
                />
                <NumberField
                  label="Wait between requests"
                  value={delay}
                  onChange={setDelay}
                  placeholder="0"
                  suffix="ms"
                />
              </div>
            </fieldset>

            <fieldset className="flex flex-wrap items-end gap-4">
              <legend className="mb-1 text-xs font-semibold tracking-[0.07em] text-fg-faint uppercase">
                Stop early — leave blank to crawl the whole site
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
                label="Beyond this many clicks from the home page"
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
                <span className="text-sm text-fg-faint">
                  Finds broken and oversized images. Slower, and it sends
                  requests to wherever the images are hosted.
                </span>
              </span>
            </label>
          </div>
        </details>

        {/* Stated rather than offered. robots.txt is honoured with no way to
            turn it off, and a crawler that gets its user blocked is a
            liability — so this is a default, not a setting. */}
        <p className="text-sm text-fg-faint">
          Pounce always obeys robots.txt, always waits when a site asks it to,
          and identifies itself in every request.
        </p>
      </div>
    </main>
  );
}
