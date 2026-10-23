import type { AdjustmentInput, Item, MovementReason, ScaleMatch, StockMovement, StockTake } from "../types";
import { call } from "./tauriClient";

/** Adds stock, writes off damage, or corrects the on-hand count. */
export function adjustStock(input: AdjustmentInput): Promise<Item> {
  return call<Item>("stock_adjust", { input });
}

export function listStockMovements(filter: {
  itemId?: number | null;
  startDate?: string | null;
  endDate?: string | null;
  reason?: MovementReason | null;
}): Promise<StockMovement[]> {
  return call<StockMovement[]>("stock_list_movements", {
    itemId: filter.itemId ?? null,
    startDate: filter.startDate ?? null,
    endDate: filter.endDate ?? null,
    reason: filter.reason ?? null,
  });
}

/** Each counted item's stock becomes the counted figure. */
export function submitStockTake(note: string | null, lines: { itemId: number; countedQty: number }[]): Promise<StockTake> {
  return call<StockTake>("stock_submit_take", { note, lines });
}

export function listStockTakes(): Promise<StockTake[]> {
  return call<StockTake[]>("stock_list_takes", {});
}

export function getStockTake(id: number): Promise<StockTake> {
  return call<StockTake>("stock_get_take", { id });
}

/** Gives the listed items (or every active item without one) an in-store
 * EAN-13 barcode; resolves to how many were assigned. */
export function generateBarcodes(itemIds: number[] | null): Promise<number> {
  return call<number>("stock_generate_barcodes", { itemIds });
}

/** A scanned weight-scale barcode, or `null` when the code isn't one. */
export function lookupScaleBarcode(code: string): Promise<ScaleMatch | null> {
  return call<ScaleMatch | null>("billing_lookup_scale_barcode", { code });
}
