/** Kitchen Order Ticket (KOT) tokens — see `src-tauri/src/db/tokens.rs`.
 *
 * A token is a kitchen-facing print (item name + qty only, no money) that is
 * separate from the bill. Tokenization is tracked per table order as a
 * running "how much of item X has already been tokenized" total, not as a
 * literal per-cart-line foreign key — `table_orders.cart_json` has no stable
 * per-line identity to hang one off of. Only the un-tokenized remainder of
 * each item ever appears as "pending". */

export interface TokenLine {
  itemId: number;
  itemName: string;
  qty: number;
  unit: string | null;
  /** Whether this line's `qty` was computed from a rupee amount rather than
   * typed directly — see `store/billingStore.ts`'s `addItemByAmount`. */
  soldByAmount: boolean;
  /** `qty × item.priceMinor`, rounded — the rupee amount this line
   * represents, computed fresh from the item's current price. Only
   * meaningful (and only shown) when `soldByAmount` is true. */
  amountMinor: number;
}

/** One counter's worth of not-yet-printed items for a table order. Items
 * whose `counterId` is null never appear here — they're intentionally
 * token-less (e.g. roti, in the client's workflow). */
export interface PendingCounterGroup {
  counterId: number;
  counterName: string;
  items: TokenLine[];
}

/** `tableOrderId`/`tableId`/`tableName` are `null` for an ad hoc (Takeaway)
 * token — a Takeaway sale has no `table_orders` row to attach one to (see
 * `printAdhocTokens`). */
export interface TokenSummary {
  id: number;
  tokenNumber: number;
  counterId: number;
  counterName: string;
  tableOrderId: number | null;
  tableId: number | null;
  tableName: string | null;
  printedAt: string;
  printedBy: number | null;
  printedByName: string | null;
  status: "printed" | "cancelled";
  items: TokenLine[];
}

/** One cart line to print an ad hoc (Takeaway) token for — see
 * `getAdhocTokenGroups`/`printAdhocTokens`. */
export interface AdhocTokenLine {
  itemId: number;
  qty: number;
}

/** Result of attempting to print one counter's token — a print (and its
 * `NothingPending` / `Failed` alternatives) is per-counter so one counter
 * failing never blocks or undoes another's. */
export type PrintOutcome =
  | { status: "printed"; token: TokenSummary }
  | { status: "nothingPending" }
  | { status: "failed"; error: string };

export interface CounterPrintResult {
  counterId: number;
  counterName: string;
  outcome: PrintOutcome;
}
