import { useCallback, useEffect, useState } from "react";

/**
 * Loads data and reloads whenever `deps` change (or `reload()` is called);
 * a slower, older request never overwrites a newer one. While `enabled` is
 * false nothing loads and the last data is kept — for a caller still
 * waiting on an input (no employee picked, a reversed date range).
 */
export function useReport<T>(load: () => Promise<T>, deps: unknown[], { enabled = true }: { enabled?: boolean } = {}) {
  const [data, setData] = useState<T | null>(null);
  const [isLoading, setIsLoading] = useState(enabled);
  const [error, setError] = useState<string | null>(null);
  const [version, setVersion] = useState(0);
  const reload = useCallback(() => setVersion((v) => v + 1), []);

  useEffect(() => {
    if (!enabled) {
      setIsLoading(false);
      return;
    }
    let current = true;
    setIsLoading(true);
    setError(null);
    load()
      .then((result) => current && setData(result))
      .catch((e: Error) => current && setError(e.message))
      .finally(() => current && setIsLoading(false));
    return () => {
      current = false;
    };
    // The caller lists what the data depends on.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, enabled, version]);

  return { data, isLoading, error, reload };
}
