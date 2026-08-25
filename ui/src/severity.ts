/// How a severity is drawn. Colour is never the only carrier: PRODUCT.md's
/// binding rule is that severity pairs a hue with an icon and a word, so a
/// reader who cannot separate red from amber still reads "Critical".
export const SEVERITY = {
  critical: { label: "Critical", icon: "●", tone: "text-critical", rank: 0 },
  warning: { label: "Warning", icon: "▲", tone: "text-warning", rank: 1 },
  notice: { label: "Notice", icon: "•", tone: "text-notice", rank: 2 },
} as const;

export type SeverityName = keyof typeof SEVERITY;

/// Unknown severities are drawn, not dropped: a file written by a newer build
/// carrying a fourth level should still show its count.
export function severity(name: string) {
  return (
    SEVERITY[name as SeverityName] ?? {
      label: name,
      icon: "•",
      tone: "text-fg-muted",
      rank: 3,
    }
  );
}
