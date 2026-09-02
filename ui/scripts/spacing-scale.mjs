/* T4.64. Counts the distinct spacing steps used across `ui/src`, the way the
   T4.63 audit did: box padding (`p-`, `px-`, `py-`) and `gap-` class tokens in
   the .tsx className strings. The point is not the total — it is how many
   *different* steps the interface asks the eye to distinguish. The audit found
   20 padding and 14 gap; a scale is a handful, not a hundred.

   `--check` fails when a token falls outside the scale, so the next ad-hoc
   value is a red build rather than a slow drift back. */
import { readdirSync, readFileSync } from "node:fs";

/* The scale, in Tailwind steps (× 0.25rem): 4, 8, 12, 16, 24, 32 px, plus 0.
   Six steps and a zero. Anything between them is a value someone typed once. */
export const SCALE = ["0", "1", "2", "3", "4", "6"];

const PAD = /\b(p[xy]?)-(\[[^\]]+\]|[0-9.]+|px)\b/g;
const GAP = /\bgap(?:-[xy])?-(\[[^\]]+\]|[0-9.]+|px)\b/g;

const collect = (re, group) => {
  const seen = new Map();
  for (const f of readdirSync("src").filter((f) => f.endsWith(".tsx"))) {
    const src = readFileSync(`src/${f}`, "utf8");
    for (const m of src.matchAll(re)) {
      const tok = m[0], val = m[group];
      const e = seen.get(tok) ?? { val, n: 0, files: new Set() };
      e.n++;
      e.files.add(f);
      seen.set(tok, e);
    }
  }
  return seen;
};

const pad = collect(PAD, 2);
const gap = collect(GAP, 1);
const offScale = [...pad, ...gap].filter(([, e]) => !SCALE.includes(e.val));

for (const [name, m] of [["padding", pad], ["gap", gap]]) {
  const uses = [...m.values()].reduce((s, e) => s + e.n, 0);
  const steps = new Set([...m.values()].map((e) => e.val));
  console.log(
    `${name}: ${steps.size} distinct steps (${[...steps].sort((a, b) => a - b).join(" ")}), ` +
      `${m.size} tokens, ${uses} uses`,
  );
  for (const [tok, e] of [...m].sort((a, b) => b[1].n - a[1].n)) {
    const flag = SCALE.includes(e.val) ? " " : "!";
    console.log(`  ${flag} ${tok.padEnd(10)} ${String(e.n).padStart(3)}x  ${[...e.files].sort().join(" ")}`);
  }
}

if (process.argv.includes("--check")) {
  if (offScale.length) {
    console.error(`\noff the scale (${SCALE.join(" ")}): ${offScale.map(([t]) => t).join(" ")}`);
    process.exit(1);
  }
  console.log(`\nevery token on the scale: ${SCALE.join(" ")}`);
}
