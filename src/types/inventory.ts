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
}

/** Everything the add/edit form submits — used for both create and update. */
export interface ItemInput {
  name: string;
  barcode: string | null;
  priceMinor: number;
  costMinor: number;
  stockQty: number;
  categoryId: number | null;
  lowStockThreshold: number;
  imagePath: string | null;
}

export interface ItemQuery {
  search?: string;
  categoryId?: number;
  includeInactive?: boolean;
}

export type DeleteOutcome = "deleted" | "archived";
