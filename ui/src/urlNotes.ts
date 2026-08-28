/// What is unusual about a URL, as words rather than four boolean columns.
///
/// None of these is an audit rule. The registry is capped at thirty for v0.1
/// and full, so these are shown where you are already looking rather than
/// promoted to findings — the same arrangement as title length beside the
/// title. They are also the mildest kind of defect: a URL with an underscore
/// is not broken, it is just harder for a search engine and a person to read.
export function urlNotes(url: string): string[] {
  // The scheme and host are not the subject: `HTTPS://` is normalised away by
  // canonicalisation, and a capital in a host is not a defect the way a
  // capital in a path is. Neither is the query, which has its own column —
  // `?ref=Twitter_x` is not an underscore in the path.
  const from = url.indexOf("/", url.indexOf("//") + 2);
  const path = (from === -1 ? "" : url.slice(from)).split(/[?#]/)[0]!;
  const notes: string[] = [];
  // Percent-escapes are uppercase hex by convention, so `%C3%A9` would report
  // capitals it does not have. Strip them before looking, and report the
  // encoding itself separately.
  if (/[A-Z]/.test(path.replace(/%[0-9A-Fa-f]{2}/g, ""))) notes.push("capitals");
  if (path.includes("_")) notes.push("underscores");
  // Stored URLs are canonical, so anything non-ASCII is already percent-encoded
  // — the `%` is the tell, and it is what a person sees pasted into a document.
  if (path.includes("%")) notes.push("encoded characters");
  if (path.split("/").length - 1 > 5) notes.push("deep path");
  return notes;
}
