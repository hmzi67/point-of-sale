/** Money fields cross IPC as integer minor units, matching the Rust side. */

export type PaymentMethod = "cash" | "card" | "other";

export interface CartLine {
  itemId: number;
  qty: number;
  /** A cashier's free-text note on this line — purely informational. */
  notes?: string | null;
}

export interface CreateSaleInput {
  items: CartLine[];
  discountMinor: number;
  taxMinor: number;
  paymentMethod: PaymentMethod;
  cashierId: number | null;
  tableId: number | null;
}

export interface SaleLine {
  itemId: number;
  itemName: string;
  qty: number;
  priceAtSaleMinor: number;
  lineTotalMinor: number;
  notes: string | null;
}

/** A completed sale plus everything a receipt needs. */
export interface Sale {
  id: number;
  subtotalMinor: number;
  discountMinor: number;
  taxMinor: number;
  totalMinor: number;
  paymentMethod: PaymentMethod;
  cashierId: number | null;
  cashierName: string | null;
  tableId: number | null;
  tableName: string | null;
  createdAt: string;
  items: SaleLine[];
}

// TableSummary / ParkedCartLine / ParkedOrder now live in ./tables — this
// module only needs `tableId` on CreateSaleInput/Sale above.
