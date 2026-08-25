import { useEffect, useState } from "react";

/// True only once `active` has been true continuously for `ms`.
///
/// The point is the *absence* of a spinner. A query that answers in 12 ms and a
/// skeleton that appears instantly produce a flash — a grey shape that arrives
/// and leaves before it can be read, which reads as a glitch rather than as
/// progress. Anything slower than a blink deserves a placeholder; anything
/// faster deserves nothing at all.
///
/// 150 ms is the default because it is roughly where a change stops feeling
/// instantaneous, and because every query M3 gates is faster than that: the
/// worst supported filter/sort pair at 1M is 220 ms, and the typical one is
/// 11–15 ms.
export function useDelayed(active: boolean, ms = 150): boolean {
  const [shown, setShown] = useState(false);
  useEffect(() => {
    if (!active) {
      setShown(false);
      return;
    }
    const id = setTimeout(() => setShown(true), ms);
    return () => clearTimeout(id);
  }, [active, ms]);
  return shown;
}
