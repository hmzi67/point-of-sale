import type { Item } from "../types";

/**
 * The item a typed or scanned code means: an exact barcode match first, a
 * short code only as a fallback — the same priority billing search uses, so
 * a short code that collides with another item's barcode never steals the
 * lookup from it.
 */
export function findItemByCode(items: Item[], code: string): Item | undefined {
  return items.find((item) => item.barcode === code) ?? items.find((item) => item.shortCode === code);
}
