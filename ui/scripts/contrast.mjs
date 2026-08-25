// Every text tier in both themes must clear WCAG AA. PRODUCT.md says the dark
// ratios were "verified, not assumed" — this is what makes that true of the
// light theme too, and what stops a later palette tweak quietly breaking one.
//
// Run: npm run check:contrast

const srgb = (hex) => {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((v) => v / 255);
};

const luminance = (hex) => {
  const [r, g, b] = srgb(hex).map((c) =>
    c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4,
  );
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
};

const ratio = (a, b) => {
  const [l1, l2] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (l1 + 0.05) / (l2 + 0.05);
};

// Body text and small labels: AA is 4.5:1. Anything used only as a large
// heading or a non-text boundary is checked at 3:1 and marked as such.
const AA = 4.5;
const AA_LARGE = 3;

const dark = {
  canvas: "#121317",
  surface: "#181A20",
  raised: "#1F2229",
  fg: "#F4F5F7",
  fgMuted: "#A2A9B5",
  fgFaint: "#838A96",
  accent: "#5E6AD2",
  accentFg: "#828CF2",
  onAccent: "#FFFFFF",
  critical: "#F2777A",
  warning: "#E5B84B",
  notice: "#56C7E0",
  pass: "#4BC98A",
};

const light = {
  canvas: "#FCFCFD",
  surface: "#FFFFFF",
  raised: "#F5F6F8",
  fg: "#16181D",
  fgMuted: "#575E6B",
  fgFaint: "#6B7280",
  accent: "#5E6AD2",
  accentFg: "#4650B0",
  onAccent: "#FFFFFF",
  critical: "#C2373C",
  warning: "#8A6208",
  notice: "#0E6C84",
  pass: "#16764C",
};

const checks = (t, name) => [
  [`${name} fg on canvas`, t.fg, t.canvas, AA],
  [`${name} fg on surface`, t.fg, t.surface, AA],
  [`${name} fg on raised`, t.fg, t.raised, AA],
  [`${name} fg-muted on canvas`, t.fgMuted, t.canvas, AA],
  [`${name} fg-muted on surface`, t.fgMuted, t.surface, AA],
  [`${name} fg-faint on canvas`, t.fgFaint, t.canvas, AA],
  [`${name} accent-fg on canvas`, t.accentFg, t.canvas, AA],
  [`${name} on-accent over accent`, t.onAccent, t.accent, AA_LARGE],
  [`${name} critical on canvas`, t.critical, t.canvas, AA],
  [`${name} warning on canvas`, t.warning, t.canvas, AA],
  [`${name} notice on canvas`, t.notice, t.canvas, AA],
  [`${name} pass on canvas`, t.pass, t.canvas, AA],
];

let failed = 0;
for (const [label, fg, bg, min] of [
  ...checks(dark, "dark"),
  ...checks(light, "light"),
]) {
  const r = ratio(fg, bg);
  const ok = r >= min;
  if (!ok) failed++;
  const tag = min === AA_LARGE ? " (3:1, non-body)" : "";
  console.log(
    `${ok ? "ok  " : "FAIL"} ${label.padEnd(34)} ${r.toFixed(2)}:1  needs ${min}${tag}`,
  );
}

if (failed) {
  console.error(`\n${failed} pair(s) below their minimum.`);
  process.exit(1);
}
console.log("\nevery pair clears its minimum.");
