/** Restaurant floor management — see `src-tauri/src/db/tables.rs`. */

export type TableStatus = "free" | "occupied" | "reserved";

export interface TableSummary {
  id: number;
  name: string;
  seats: number;
  status: TableStatus;
  hasParkedOrder: boolean;
  /** The open `table_orders.id` behind this table, if any — pass this (not
   * the table id) to the `tokens_*` commands, since a token belongs to the
   * order and follows it across a table shift. `null` exactly when
   * `hasParkedOrder` is `false`. */
  openOrderId: number | null;
}

export interface ParkedCartLine {
  itemId: number;
  qty: number;
}

/** The cart currently parked on a table — resumed into the billing cart. */
export interface ParkedOrder {
  items: ParkedCartLine[];
  discountMinor: number;
}

/** Both tables' post-shift state — see `shiftTableOrder` in
 * `services/tablesService.ts`. */
export interface ShiftTableResult {
  from: TableSummary;
  to: TableSummary;
}
