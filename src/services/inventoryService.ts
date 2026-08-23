import type { Category, DeleteOutcome, Item, ItemInput, ItemQuery, ImportSummary } from "../types";
import { call } from "./tauriClient";

export function getItems(query: ItemQuery = {}): Promise<Item[]> {
  return call<Item[]>("inventory_get_items", { query });
}

export function addItem(input: ItemInput): Promise<Item> {
  return call<Item>("inventory_add_item", { input });
}

export function updateItem(id: number, input: ItemInput): Promise<Item> {
  return call<Item>("inventory_update_item", { id, input });
}

/** Deletes outright unless the item has sale history, in which case it is archived. */
export function deleteItem(id: number): Promise<DeleteOutcome> {
  return call<DeleteOutcome>("inventory_delete_item", { id });
}

export function getCategories(): Promise<Category[]> {
  return call<Category[]>("inventory_get_categories", {});
}

export function addCategory(name: string): Promise<Category> {
  return call<Category>("inventory_add_category", { name });
}

/**
 * Uploads a product photo. `dataBase64` is the raw base64 payload (no
 * `data:...;base64,` prefix — strip that client-side first) and `extension`
 * is the file extension without a dot (`"png"`, `"jpg"`, …). Returns the
 * filename to store as `ItemInput.imagePath`.
 */
export function uploadItemImage(dataBase64: string, extension: string): Promise<string> {
  return call<string>("inventory_upload_image", { dataBase64, extension });
}

/** Reads a stored image back as a `data:` URL, ready for an `<img src>`. */
export function getItemImage(fileName: string): Promise<string> {
  return call<string>("inventory_get_image", { fileName });
}

/** Bulk-loads a stock list from a CSV file's raw text content. Imports what
 * it can and reports the rest — see `ImportSummary`. */
export function importItemsCsv(csvContent: string): Promise<ImportSummary> {
  return call<ImportSummary>("inventory_import_csv", { csvContent });
}

/** A ready-to-download example CSV, so a shop owner knows the expected
 * columns before building their own file. */
export function getCsvTemplate(): Promise<string> {
  return call<string>("inventory_csv_template", {});
}

/** Item ids currently qualifying as a "best seller" — ranked by quantity
 * sold in the last `periodDays`, capped at `limit`. Recomputed fresh on every
 * call (never stored), so the fire badge can never go stale; a shop with too
 * little sales history to clear the minimum-quantity floor gets an empty
 * list rather than an arbitrary one. */
export function getBestSellingItemIds(periodDays: number, limit: number): Promise<number[]> {
  return call<number[]>("inventory_get_best_selling_item_ids", { periodDays, limit });
}
