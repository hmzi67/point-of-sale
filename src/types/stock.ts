import type { Item } from "./inventory";

/** Stock control outside selling — see `db/stock.rs`. */

export type AdjustmentKind = "add" | "damage" | "correction";

export interface AdjustmentInput {
  itemId: number;
  kind: AdjustmentKind;
  /** add/damage: how many in/out; correction: the new on-hand count. */
  qty: number;
  note: string | null;
}

export type MovementReason = "grn" | AdjustmentKind | "stock_take";

export interface StockMovement {
  id: number;
  itemId: number;
  itemName: string;
  unit: string | null;
  qtyChange: number;
  reason: MovementReason;
  referenceId: number | null;
  note: string | null;
  userName: string | null;
  createdAt: string;
  valueMinor: number;
}

export interface StockTakeLine {
  itemId: number;
  itemName: string;
  unit: string | null;
  expectedQty: number;
  countedQty: number;
  varianceQty: number;
  varianceValueMinor: number;
}

export interface StockTake {
  id: number;
  note: string | null;
  createdByName: string | null;
  createdAt: string;
  itemsCounted: number;
  itemsWithVariance: number;
  varianceValueMinor: number;
  lines: StockTakeLine[];
}

/** A weight-scale barcode resolved to an item and the quantity it encodes. */
export interface ScaleMatch {
  item: Item;
  qty: number;
}
