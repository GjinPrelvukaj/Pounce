// Every text tier in both themes must clear WCAG AA. PRODUCT.md says the dark
// ratios were "verified, not assumed" — this is what makes that true of the
// light theme too, and what stops a later palette tweak quietly breaking one.
//
// Run: npm run check:contrast

// Colours are authored in OKLCH (the token file is the source of truth), so
// this converts OKLCH -> OKLab -> LMS -> linear sRGB and takes luminance from
// the linear values directly. Hex is still accepted so a one-off comparison
// can be pasted in without converting it first. An unparseable colour throws
// rather than scoring zero — a checker that silently grades garbage is worse
// than no checker, which this script has already learned once.
const HEX = /^#([0-9a-f]{6})$/i;
const OKLCH = /^oklch\(\s*([\d.]+)%\s+([\d.]+)\s+([\d.]+)\s*\)$/i;

const linearFromHex = (hex) => {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255]
    .map((v) => v / 255)
    .map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
};

const linearFromOklch = (L, C, H) => {
  const h = (H * Math.PI) / 180;
  const a = C * Math.cos(h);
  const b = C * Math.sin(h);
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
  ].map((v) => Math.min(1, Math.max(0, v)));
};

const linear = (color) => {
  if (HEX.test(color)) return linearFromHex(color);
  const ok = OKLCH.exec(color);
  if (ok) return linearFromOklch(+ok[1] / 100, +ok[2], +ok[3]);
  throw new Error(`unparseable colour: ${color}`);
};

const luminance = (color) => {
  const [r, g, b] = linear(color);
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
  canvas: "oklch(20.5% 0.007 283.5)",
  surface: "oklch(23.5% 0.008 283.5)",
  raised: "oklch(27.0% 0.009 283.5)",
  fg: "oklch(95.0% 0.006 283.5)",
  fgMuted: "oklch(71.5% 0.012 283.5)",
  fgFaint: "oklch(62.5% 0.012 283.5)",
  accent: "oklch(55.4% 0.2358 283.5)",
  accentFg: "oklch(74.4% 0.1421 290.3)",
  onAccent: "oklch(99.0% 0.003 283.5)",
  critical: "oklch(77.9% 0.1133 26.6)",
  warning: "oklch(80.3% 0.1347 86.2)",
  notice: "oklch(78.1% 0.1042 213.7)",
  pass: "oklch(75.9% 0.1375 158.7)",
};

const light = {
  canvas: "oklch(96.8% 0.005 283.5)",
  surface: "oklch(99.2% 0.003 283.5)",
  raised: "oklch(94.6% 0.006 283.5)",
  fg: "oklch(23.5% 0.010 283.5)",
  fgMuted: "oklch(45.0% 0.014 283.5)",
  fgFaint: "oklch(53.5% 0.014 283.5)",
  accent: "oklch(55.4% 0.2358 283.5)",
  accentFg: "oklch(49.5% 0.2176 283.2)",
  onAccent: "oklch(99.0% 0.003 283.5)",
  critical: "oklch(53.5% 0.1654 27.0)",
  warning: "oklch(51.6% 0.1031 82.2)",
  notice: "oklch(45.1% 0.0793 221.9)",
  pass: "oklch(46.1% 0.1073 153.8)",
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
