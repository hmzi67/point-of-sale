import type { Category, DeleteOutcome, Item, ItemInput, ItemQuery } from "../types";
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
