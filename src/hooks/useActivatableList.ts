import { useCallback, useEffect, useState } from "react";

/** A management list whose rows are deactivated rather than deleted (staff
 * accounts, employees): loads on mount, `reload()` after an edit, and
 * `toggleActive(row)` flips one row with a per-row busy flag. Errors from
 * either land in `error`. `load`/`setActive` should be stable (module-level
 * service functions are). */
export function useActivatableList<T extends { id: number; isActive: boolean }>(
  load: () => Promise<T[]>,
  setActive: (id: number, isActive: boolean) => Promise<unknown>,
) {
  const [items, setItems] = useState<T[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<number | null>(null);

  const reload = useCallback(() => {
    setIsLoading(true);
    setError(null);
    load()
      .then(setItems)
      .catch((e: Error) => setError(e.message))
      .finally(() => setIsLoading(false));
  }, [load]);

  useEffect(reload, [reload]);

  const toggleActive = async (row: T) => {
    setBusyId(row.id);
    setError(null);
    try {
      await setActive(row.id, !row.isActive);
      reload();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusyId(null);
    }
  };

  return { items, isLoading, error, busyId, reload, toggleActive };
}
