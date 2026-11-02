import type { CreateSaleInput, Item, Sale, SaleListItem } from "../types";
import { call } from "./tauriClient";

/** Active items matching `query` by name or barcode, exact-barcode-first. */
export function searchItems(query: string): Promise<Item[]> {
  return call<Item[]>("billing_search_items", { query });
}

export function createSale(input: CreateSaleInput): Promise<Sale> {
  return call<Sale>("billing_create_sale", { input });
}

/** The number the next order gets in the signed-in cashier's open shift (1
 * at the start of each shift) — `null` while no shift is open. */
export function getNextOrderNo(): Promise<number | null> {
  return call<number | null>("billing_next_order_no", {});
}

/** Fetches a completed sale — used for receipt reprint. */
export function getSale(id: number): Promise<Sale> {
  return call<Sale>("billing_get_sale", { id });
}

/** Prints to this installation's configured thermal printer (USB,
 * auto-detected, on desktop; the paired Bluetooth printer selected in
 * Settings, on Android) — rejects if none is set up/reachable, callers
 * should catch and fall back to the PDF receipt. */
export function printReceiptThermal(saleId: number): Promise<void> {
  return call<void>("billing_print_receipt_thermal", { saleId });
}

/** The most recent sales, newest first — the refund flow's "pick the
 * original sale" list. */
export function listRecentSales(limit: number): Promise<SaleListItem[]> {
  return call<SaleListItem[]>("billing_list_recent_sales", { limit });
}
