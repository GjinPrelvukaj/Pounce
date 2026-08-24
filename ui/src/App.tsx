import { useEffect, useState } from "react";
import { engineInfo, type EngineInfo } from "./engine";

/// The shell, and nothing else yet: T4.5 onward fill it in. What it proves
/// today is the only thing a scaffold can prove — that the window opens and
/// the frontend can reach the engine across the Tauri boundary.
export default function App() {
  const [info, setInfo] = useState<EngineInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    engineInfo().then(setInfo).catch((e: unknown) => setError(String(e)));
  }, []);

  return (
    <main className="flex h-full flex-col items-center justify-center gap-3 bg-white text-neutral-900 dark:bg-neutral-950 dark:text-neutral-100">
      <h1 className="text-2xl font-semibold tracking-tight">Pounce</h1>
      {error ? (
        <p className="font-mono text-sm text-red-600 dark:text-red-400">{error}</p>
      ) : info ? (
        <p className="font-mono text-sm text-neutral-500 dark:text-neutral-400">
          engine {info.version} · schema {info.schemaVersion} · {info.rules} rules
        </p>
      ) : (
        <p className="font-mono text-sm text-neutral-400">connecting…</p>
      )}
    </main>
  );
}
