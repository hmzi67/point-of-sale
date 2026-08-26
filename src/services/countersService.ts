import type { Counter } from "../types";
import { call } from "./tauriClient";

/** All counters; pass `includeInactive` to also get deactivated ones (for a
 * management screen — the item-form dropdown should omit them). */
export function getCounters(includeInactive = false): Promise<Counter[]> {
  return call<Counter[]>("counters_get_counters", { includeInactive });
}

export function addCounter(name: string): Promise<Counter> {
  return call<Counter>("counters_add_counter", { name });
}

export function updateCounter(id: number, name: string): Promise<Counter> {
  return call<Counter>("counters_update_counter", { id, name });
}

/** Deactivates or reactivates a counter — never a hard delete, since items
 * may reference it. */
export function setCounterActive(id: number, isActive: boolean): Promise<Counter> {
  return call<Counter>("counters_set_active", { id, isActive });
}
