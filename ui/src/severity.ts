/// How a severity is drawn. Colour is never the only carrier: PRODUCT.md's
/// binding rule is that severity pairs a hue with an icon and a word, so a
/// reader who cannot separate red from amber still reads "Critical".
/// `type` and `priority` are the words a client-facing report uses, and they
/// map 1:1 onto our three severities because the severity definitions in
/// `pounce-audit` already say exactly this: Critical means assume the page is
/// broken, Warning means a real defect on a page that otherwise works, Notice
/// means nothing is wrong and there is only headroom.
export const SEVERITY = {
  critical: {
    label: "Critical",
    type: "Issue",
    priority: "High",
    icon: "●",
    tone: "text-critical",
    dim: "bg-critical-dim",
    rank: 0,
  },
  warning: {
    label: "Warning",
    type: "Warning",
    priority: "Medium",
    icon: "▲",
    tone: "text-warning",
    dim: "bg-warning-dim",
    rank: 1,
  },
  notice: {
    label: "Notice",
    type: "Opportunity",
    priority: "Low",
    icon: "•",
    tone: "text-notice",
    dim: "bg-notice-dim",
    rank: 2,
  },
} as const;

export type SeverityName = keyof typeof SEVERITY;

/// Unknown severities are drawn, not dropped: a file written by a newer build
/// carrying a fourth level should still show its count.
export function severity(name: string) {
  return (
    SEVERITY[name as SeverityName] ?? {
      label: name,
      type: name,
      priority: "Low",
      icon: "•",
      tone: "text-fg-muted",
      dim: "bg-raised-2",
      rank: 3,
    }
  );
}
