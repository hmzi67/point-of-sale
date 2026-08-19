import type { ParkedCartLine, ParkedOrder, TableSummary } from "../types";
import { call } from "./tauriClient";

/** Every table with status, for the floor view and the billing table picker. */
export function getTables(): Promise<TableSummary[]> {
  return call<TableSummary[]>("tables_get_tables", {});
}

/** Adds a new physical table to the floor. Owner/admin only. */
export function addTable(name: string, seats: number): Promise<TableSummary> {
  return call<TableSummary>("tables_add_table", { name, seats });
}

/** Directly sets a table's status (e.g. marking one reserved). */
export function updateTableStatus(tableId: number, status: TableSummary["status"]): Promise<TableSummary> {
  return call<TableSummary>("tables_update_table_status", { tableId, status });
}

/** Seats a table: starts an empty draft order (if none is open yet) and
 * marks it occupied. Idempotent — safe to call on an already-occupied table. */
export function assignOrderToTable(tableId: number): Promise<TableSummary> {
  return call<TableSummary>("tables_assign_order_to_table", { tableId });
}

/** Manually releases a table — cancels its open order (if any) and frees it. */
export function clearTable(tableId: number): Promise<TableSummary> {
  return call<TableSummary>("tables_clear_table", { tableId });
}

/** Parks the current cart on a table ("Save to table") instead of completing. */
export function attachCartToTable(
  tableId: number,
  items: ParkedCartLine[],
  discountMinor: number,
): Promise<void> {
  return call<void>("tables_attach_cart_to_table", { tableId, items, discountMinor });
}

/** The cart parked on a table, if any — used to resume billing it. */
export function getParkedCart(tableId: number): Promise<ParkedOrder | null> {
  return call<ParkedOrder | null>("tables_get_parked_cart", { tableId });
}
