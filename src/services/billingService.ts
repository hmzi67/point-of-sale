import type { CreateSaleInput, Item, ParkedCartLine, ParkedOrder, Sale, TableSummary } from "../types";
import { call } from "./tauriClient";

/** Active items matching `query` by name or barcode, exact-barcode-first. */
export function searchItems(query: string): Promise<Item[]> {
  return call<Item[]>("billing_search_items", { query });
}

export function createSale(input: CreateSaleInput): Promise<Sale> {
  return call<Sale>("billing_create_sale", { input });
}

/** Fetches a completed sale — used for receipt reprint. */
export function getSale(id: number): Promise<Sale> {
  return call<Sale>("billing_get_sale", { id });
}

/** Tables with status, for the table picker. Only call when the `tables`
 * module is enabled. */
export function getTables(): Promise<TableSummary[]> {
  return call<TableSummary[]>("billing_get_tables", {});
}

/** Parks the current cart on a table ("Save to table") instead of completing. */
export function attachCartToTable(
  tableId: number,
  items: ParkedCartLine[],
  discountMinor: number,
): Promise<void> {
  return call<void>("billing_attach_cart_to_table", { tableId, items, discountMinor });
}

/** The cart parked on a table, if any — used to resume billing it. */
export function getParkedCart(tableId: number): Promise<ParkedOrder | null> {
  return call<ParkedOrder | null>("billing_get_parked_cart", { tableId });
}

/** Always rejects today (no thermal printer wired up) — callers should catch
 * and fall back to the PDF receipt. */
export function printReceiptThermal(saleId: number): Promise<void> {
  return call<void>("billing_print_receipt_thermal", { saleId });
}
