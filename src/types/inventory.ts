/**
 * Money fields cross IPC as integer minor units (paisa), matching the Rust
 * side and CLAUDE.md's convention — never floats. Components convert to/from
 * a decimal amount only in the add/edit form, at the input boundary.
 */

export interface Category {
  id: number;
  name: string;
}

export interface Item {
  id: number;
  name: string;
  barcode: string | null;
  /** Short (2-4 digit) PLU-style code for keyboard-first billing — a cashier
   * types this into the billing screen's quick-entry buffer instead of
   * scanning. `null` for most items until an Owner/Admin sets one; such an
   * item just stays reachable only via search/tap/barcode, as before. */
  shortCode: string | null;
  /** Short blurb shown on the billing screen's item detail modal. */
  description: string | null;
  priceMinor: number;
  costMinor: number;
  stockQty: number;
  categoryId: number | null;
  categoryName: string | null;
  lowStockThreshold: number;
  isActive: boolean;
  /** `stockQty <= lowStockThreshold`, computed on the Rust side. */
  isLowStock: boolean;
  /** Filename under the product-image store, or `null` if no photo is set.
   * Fetch the displayable data URL with `inventoryService.getItemImage`. */
  imagePath: string | null;
  /** Sold by typing a rupee amount rather than a quantity on the billing
   * screen (loose groceries — channa, rice, dry fruits) — the billing
   * screen divides the amount by `priceMinor` to get the qty. Not every
   * item qualifies (a bottled soft drink doesn't), so this is opt-in per
   * item, set from this add/edit form. */
  soldByAmount: boolean;
  /** Display unit for this item's quantity (e.g. "kg"), shown on the cart
   * line and receipt. `null` for a normal per-piece item. */
  unit: string | null;
  /** The kitchen/prep counter this item's KOT token prints to, or `null` if
   * this item doesn't need one (e.g. roti, in a client's workflow — see
   * `Counter` and the Tables module's token feature). */
  counterId: number | null;
  counterName: string | null;
}

/** Everything the add/edit form submits — used for both create and update. */
export interface ItemInput {
  name: string;
  barcode: string | null;
  shortCode: string | null;
  description: string | null;
  priceMinor: number;
  costMinor: number;
  stockQty: number;
  categoryId: number | null;
  lowStockThreshold: number;
  imagePath: string | null;
  soldByAmount: boolean;
  unit: string | null;
  /** Counter this item's token prints to, or `null` for a token-less item. */
  counterId: number | null;
}

/** Result of a bulk CSV import — see `db::csv_import` on the Rust side. */
export interface ImportRowError {
  /** 1-based, counting the header as row 1 — matches what a spreadsheet
   * program shows for that row. */
  row: number;
  message: string;
}

export interface ImportSummary {
  imported: number;
  errors: ImportRowError[];
}

export interface ItemQuery {
  search?: string;
  categoryId?: number;
  includeInactive?: boolean;
}

export type DeleteOutcome = "deleted" | "archived";
