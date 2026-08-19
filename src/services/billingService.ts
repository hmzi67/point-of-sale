import type { CreateSaleInput, Item, Sale } from "../types";
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

/** Always rejects today (no thermal printer wired up) — callers should catch
 * and fall back to the PDF receipt. */
export function printReceiptThermal(saleId: number): Promise<void> {
  return call<void>("billing_print_receipt_thermal", { saleId });
}
