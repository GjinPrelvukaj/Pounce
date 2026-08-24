/// The three theme states, and the one rule that makes them three rather than
/// two: **system is a live state, not a starting value.** Choosing "system"
/// means the window follows the OS from then on, including when the user flips
/// their Mac to dark at sunset while Pounce is open mid-crawl.

export type ThemeChoice = "light" | "dark" | "system";
export type ResolvedTheme = "light" | "dark";

const STORAGE_KEY = "pounce.theme";

/// Reads the OS preference. Everything else here is bookkeeping around this.
export function systemTheme(): ResolvedTheme {
  return window.matchMedia("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}

export function storedChoice(): ThemeChoice {
  const raw = localStorage.getItem(STORAGE_KEY);
  return raw === "light" || raw === "dark" || raw === "system" ? raw : "system";
}

export function resolve(choice: ThemeChoice): ResolvedTheme {
  return choice === "system" ? systemTheme() : choice;
}

/// Applies a choice by *removing* the attribute for "system" and setting it
/// otherwise.
///
/// Removing rather than resolving-and-setting is the point: with no attribute,
/// the `prefers-color-scheme` rule in the stylesheet governs, so the OS theme
/// is followed natively — no listener, no repaint, and correct before any
/// script has run. JavaScript is only involved when the user has overridden.
export function apply(choice: ThemeChoice): ResolvedTheme {
  if (choice === "system") {
    delete document.documentElement.dataset.theme;
  } else {
    document.documentElement.dataset.theme = choice;
  }
  return resolve(choice);
}

export function setChoice(choice: ThemeChoice): ResolvedTheme {
  localStorage.setItem(STORAGE_KEY, choice);
  return apply(choice);
}

/// Calls back whenever the OS preference changes *and* the user is on
/// "system". Returns its own unsubscribe.
export function watchSystem(onChange: (resolved: ResolvedTheme) => void) {
  const query = window.matchMedia("(prefers-color-scheme: dark)");
  const handler = () => {
    if (storedChoice() === "system") onChange(apply("system"));
  };
  query.addEventListener("change", handler);
  return () => query.removeEventListener("change", handler);
}
