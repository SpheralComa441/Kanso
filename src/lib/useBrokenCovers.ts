import { useCallback, useState } from "react";

/**
 * Remembers cover URLs that failed to load, so the UI can fall back to the titled
 * placeholder instead of an empty frame. A replaced cover gets a new URL (its
 * cache-busting stamp changes) and is therefore tried again.
 */
export function useBrokenCovers() {
  const [broken, setBroken] = useState<ReadonlySet<string>>(() => new Set());

  const markBroken = useCallback((url: string) => {
    setBroken((prev) => (prev.has(url) ? prev : new Set(prev).add(url)));
  }, []);

  /** `url` if it is still believed to load, `null` otherwise. */
  const usableCover = useCallback(
    (url: string | null) => (url && !broken.has(url) ? url : null),
    [broken]
  );

  return { usableCover, markBroken };
}
