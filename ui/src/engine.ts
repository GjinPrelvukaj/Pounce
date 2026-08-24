import { invoke } from "@tauri-apps/api/core";

/// Mirrors `pounce-app`'s `EngineInfo`. Tauri serialises commands as JSON, so
/// nothing checks these two definitions against each other at build time —
/// keep the shapes together and change them together.
export type EngineInfo = {
  version: string;
  schemaVersion: number;
  rules: number;
};

/// True when the page is running inside the desktop shell rather than a bare
/// browser. `npm run dev` on its own serves the UI over http, where there is no
/// engine at all — worth saying plainly, because the raw failure is a
/// `TypeError` about `invoke` that reads like a bug in the app.
export function inDesktopShell(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

export async function engineInfo(): Promise<EngineInfo> {
  if (!inDesktopShell()) {
    throw new Error(
      "no engine here — this page is running in a browser. Use `cargo run -p pounce-app`.",
    );
  }
  return invoke<EngineInfo>("engine_info");
}
