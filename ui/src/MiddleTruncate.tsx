/// A URL that loses its middle rather than its end.
///
/// `text-overflow: ellipsis` cuts the tail, which on a crawl of
/// `/baseball/new-jersey/atlantic-county/absecon` throws away the only part
/// that distinguishes the row from its thousand siblings and keeps the part
/// every row shares.
///
/// No measurement: the head is an ordinary truncating box and the tail is a
/// `shrink-0` box beside it, so flexbox does the arithmetic at layout time and
/// stays right through a window resize.
export function MiddleTruncate({
  text,
  tailLength = 28,
}: {
  text: string;
  tailLength?: number;
}) {
  const lastSlash = text.lastIndexOf("/");
  // Prefer splitting at the last path separator, so the tail is a whole
  // segment; fall back to a fixed number of characters when that segment is
  // longer than the space it would claim.
  const keep =
    lastSlash > 0 && text.length - lastSlash <= tailLength
      ? text.length - lastSlash
      : tailLength;

  if (keep >= text.length) {
    return <span className="block truncate">{text}</span>;
  }

  return (
    <span className="flex min-w-0">
      <span className="truncate">{text.slice(0, text.length - keep)}</span>
      <span className="shrink-0 whitespace-pre">{text.slice(-keep)}</span>
    </span>
  );
}
