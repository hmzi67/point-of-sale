import { useCallback, useEffect, useState } from "react";
import { getBestSellingItemIds } from "../services/inventoryService";

/** Rolling window (days) and how many items badge as "best seller" — a
 * sensible default a client can outgrow (a slow-moving shop may want a
 * longer window), not a hardcoded constant baked into the query itself; see
 * `reports::get_best_selling_item_ids` for the actual floor/window logic. */
const DEFAULT_PERIOD_DAYS = 30;
const DEFAULT_LIMIT = 5;

/**
 * The current best-seller item id set, recomputed live from `sale_items`
 * every time it's fetched — never cached across screens or stored as a flag,
 * so it can't go stale. Shared by Inventory and Billing so both screens badge
 * the same items the same way.
 */
export function useBestSellerIds(periodDays = DEFAULT_PERIOD_DAYS, limit = DEFAULT_LIMIT) {
  const [ids, setIds] = useState<Set<number>>(new Set());

  const reload = useCallback(() => {
    void getBestSellingItemIds(periodDays, limit)
      .then((result) => setIds(new Set(result)))
      .catch(() => {
        // Best-effort — a failure here just means no badges this load,
        // never a reason to break the Inventory/Billing screen itself.
      });
  }, [periodDays, limit]);

  useEffect(() => {
    reload();
  }, [reload]);

  return { bestSellerIds: ids, reloadBestSellers: reload };
}
