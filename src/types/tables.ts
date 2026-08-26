/** Restaurant floor management — see `src-tauri/src/db/tables.rs`. */

export type TableStatus = "free" | "occupied" | "reserved";

export interface TableSummary {
  id: number;
  name: string;
  seats: number;
  status: TableStatus;
  hasParkedOrder: boolean;
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
