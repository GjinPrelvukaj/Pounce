import { useEffect, useState } from "react";
import { engineInfo, type EngineInfo } from "./engine";
import {
  resolve,
  setChoice,
  storedChoice,
  watchSystem,
  type ThemeChoice,
} from "./theme";

const THEMES: ThemeChoice[] = ["light", "dark", "system"];

/// The shell. T4.5 onward replace the body of this with real screens; what is
/// here now is the smallest surface that exercises every token, so both themes
/// can be looked at rather than asserted.
export default function App() {
  const [info, setInfo] = useState<EngineInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [choice, setChoiceState] = useState<ThemeChoice>(storedChoice);
  const [resolved, setResolved] = useState(() => resolve(storedChoice()));

  useEffect(() => {
    engineInfo()
      .then(setInfo)
      .catch((e: unknown) => setError(String(e)));
  }, []);

  // Only fires while the user is on "system"; the stylesheet has already
  // repainted by the time this runs — it exists so the label agrees with the
  // window.
  useEffect(() => watchSystem(setResolved), []);

  function pick(next: ThemeChoice) {
    setChoiceState(next);
    setResolved(setChoice(next));
  }

  return (
    <div className="flex h-full flex-col bg-canvas text-fg">
      <header className="flex items-center justify-between border-b border-border bg-surface px-4 py-2.5">
        <div className="flex items-baseline gap-2">
          <span className="text-lg font-semibold tracking-tight">Pounce</span>
          {info && (
            <span className="tabular text-xs text-fg-faint">
              engine {info.version} · schema {info.schemaVersion} · {info.rules}{" "}
              rules
            </span>
          )}
          {error && (
            <span className="tabular text-xs text-critical">{error}</span>
          )}
        </div>
        <ThemePicker choice={choice} resolved={resolved} onPick={pick} />
      </header>

      <main className="flex-1 overflow-auto p-4">
        <Palette />
      </main>
    </div>
  );
}

function ThemePicker({
  choice,
  resolved,
  onPick,
}: {
  choice: ThemeChoice;
  resolved: string;
  onPick: (t: ThemeChoice) => void;
}) {
  return (
    <div className="flex items-center gap-2">
      <span className="tabular text-xs text-fg-faint">
        {choice === "system" ? `system · ${resolved}` : resolved}
      </span>
      <div className="flex rounded-md border border-border bg-raised p-0.5">
        {THEMES.map((t) => (
          <button
            key={t}
            onClick={() => onPick(t)}
            aria-pressed={choice === t}
            className={`rounded-sm px-2 py-1 text-xs capitalize transition-colors duration-150 ease-state ${
              choice === t
                ? "bg-accent text-on-accent"
                : "text-fg-muted hover:text-fg"
            }`}
          >
            {t}
          </button>
        ))}
      </div>
    </div>
  );
}

/// Scaffolding, not a screen: every token rendered once so a theme change is
/// visible and a broken variable is obvious. Delete when T4.9's grid arrives.
function Palette() {
  // Written out rather than built with a template string: Tailwind generates
  // utilities by scanning source text, so `bg-${name}-dim` produces a class
  // that exists in the DOM and in no stylesheet.
  const severities = [
    {
      icon: "●",
      label: "Critical",
      className: "bg-critical-dim text-critical",
    },
    { icon: "▲", label: "Warning", className: "bg-warning-dim text-warning" },
    { icon: "◆", label: "Notice", className: "bg-notice-dim text-notice" },
    { icon: "✓", label: "Pass", className: "bg-pass-dim text-pass" },
  ] as const;

  const surfaces = [
    { name: "canvas", className: "bg-canvas" },
    { name: "surface", className: "bg-surface" },
    { name: "raised", className: "bg-raised" },
    { name: "raised-2", className: "bg-raised-2" },
  ] as const;

  return (
    <div className="flex flex-col gap-4">
      <section className="rounded-md border border-border bg-surface p-3">
        <h2 className="mb-2 text-sm font-medium text-fg-muted">
          Severity — never colour alone
        </h2>
        <div className="flex flex-wrap gap-2">
          {severities.map((s) => (
            <span
              key={s.label}
              className={`flex items-center gap-1.5 rounded-sm px-2 py-1 text-xs ${s.className}`}
            >
              <span aria-hidden>{s.icon}</span>
              {s.label}
            </span>
          ))}
        </div>
        <p className="mt-2 text-xs text-fg-faint">
          Each pairs an icon and a word with its colour, so severity survives
          both themes and a reader who cannot separate the hues.
        </p>
      </section>

      <section className="rounded-md border border-border bg-surface p-3">
        <h2 className="mb-2 text-sm font-medium text-fg-muted">Surfaces</h2>
        <div className="flex flex-wrap gap-2">
          {surfaces.map((s) => (
            <div
              key={s.name}
              className={`flex h-14 w-28 items-end rounded-sm border border-border p-1.5 ${s.className}`}
            >
              <span className="tabular text-xs text-fg-faint">{s.name}</span>
            </div>
          ))}
        </div>
      </section>

      <section className="rounded-md border border-border bg-surface p-3">
        <h2 className="mb-2 text-sm font-medium text-fg-muted">
          Type — Inter for UI, JetBrains Mono for every measurement
        </h2>
        <p className="text-fg">Primary text at the interface's base size.</p>
        <p className="text-fg-muted">Secondary, for supporting labels.</p>
        <p className="text-fg-faint">Tertiary, for the quietest metadata.</p>
        <p className="tabular mt-2 text-fg">
          1,048,576 URLs · 11.3 ms · 99.7% · 000111222
        </p>
        <p className="tabular text-accent-fg">
          https://example.com/section/word-4821
        </p>
      </section>
    </div>
  );
}
