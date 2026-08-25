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
  canvas: "#1A1815",
  surface: "#211E1A",
  raised: "#292521",
  fg: "#F2EFE9",
  fgMuted: "#ADA69B",
  fgFaint: "#968F83",
  accent: "#6A4DF4",
  accentFg: "#AC9BFF",
  onAccent: "#FFFFFF",
  critical: "#F79A90",
  warning: "#E5B84B",
  notice: "#5CC9E0",
  pass: "#55CB90",
};

const light = {
  canvas: "#F6F4F0",
  surface: "#FFFEFC",
  raised: "#F1EEE8",
  fg: "#21201C",
  fgMuted: "#57534A",
  fgFaint: "#6C665B",
  accent: "#6A4DF4",
  accentFg: "#5A3FD6",
  onAccent: "#FFFFFF",
  critical: "#BA3A34",
  warning: "#85610A",
  notice: "#0D5F75",
  pass: "#15693B",
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
