/// The colour a response code is drawn in — one definition, for every table
/// that draws one.
///
/// There were three copies and they disagreed: a 404 was amber in the page
/// grid and red in the images grid, which makes the same fact look like two
/// different severities depending on which tab you are on. The rule is the
/// severity the code carries, not the table it appears in.
///
/// Never colour alone, per DESIGN.md: the code itself is the label, which is
/// why these are safe to tint. A colour with no number beside it would not be.
export function statusTone(status: number): string {
  if (status >= 500) return "text-critical";
  if (status >= 400) return "text-warning";
  if (status >= 300) return "text-notice";
  return "text-pass";
}
