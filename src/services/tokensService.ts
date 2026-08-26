import type { AdhocTokenLine, CounterPrintResult, PendingCounterGroup, TokenSummary } from "../types";
import { call } from "./tauriClient";

/** Cart lines for a table order that have NOT yet been tokenized, grouped by
 * counter. Items with no counter assigned are excluded entirely — they never
 * appear on any token. */
export function getPendingTokenItems(tableOrderId: number): Promise<PendingCounterGroup[]> {
  return call<PendingCounterGroup[]>("tokens_get_pending_items", { tableOrderId });
}

/** Tokens already printed for this table order, most recent first — for the
 * "previously printed" list and reprint. */
export function getTokensForOrder(tableOrderId: number): Promise<TokenSummary[]> {
  return call<TokenSummary[]>("tokens_get_for_order", { tableOrderId });
}

/** Prints one token per selected counter, each for that counter's currently
 * pending items. Each counter's outcome is independent — one counter's print
 * failure never blocks or undoes another's, and a counter with nothing
 * pending is reported rather than silently skipped. */
export function printTokens(tableOrderId: number, counterIds: number[]): Promise<CounterPrintResult[]> {
  return call<CounterPrintResult[]>("tokens_print", { tableOrderId, counterIds });
}

/** Reprints an existing token unchanged, marked "REPRINT" on the output —
 * for a lost/damaged token, not for new items. */
export function reprintToken(tokenId: number): Promise<void> {
  return call<void>("tokens_reprint", { tokenId });
}

/** What would print right now for a Takeaway cart, grouped by counter — the
 * ad hoc (no table order) counterpart of `getPendingTokenItems`. Always
 * reports the full quantity of every counter-eligible line in `items`, not
 * a delta — there's no persisted order to diff against for a Takeaway sale. */
export function getAdhocTokenGroups(items: AdhocTokenLine[]): Promise<PendingCounterGroup[]> {
  return call<PendingCounterGroup[]>("tokens_get_adhoc_groups", { items });
}

/** Prints tokens for a Takeaway cart that has no table (and so no parked
 * order) behind it at all. Unlike `printTokens`, every call sends the full
 * quantity of each line, not just what's new — see `getAdhocTokenGroups`'s
 * doc comment. */
export function printAdhocTokens(items: AdhocTokenLine[], counterIds: number[]): Promise<CounterPrintResult[]> {
  return call<CounterPrintResult[]>("tokens_print_adhoc", { items, counterIds });
}
