/** Money fields cross IPC as integer minor units, matching the Rust side. */

export type PaymentMethod = "cash" | "card" | "other" | "platform";

/** What a completed sale's `paymentMethod` may be: one of the above, or
 * `split` (several tenders) / `credit` (all pay-later). */
export type SalePaymentMethod = PaymentMethod | "split" | "credit";

/** How the cashier is taking payment — a single method, several
 * (`split`), or pay-later (`credit`). */
export type CheckoutMode = PaymentMethod | "split" | "credit";

/** One tender of a sale; `credit` is the pay-later part. */
export interface PaymentPart {
  method: "cash" | "card" | "other" | "platform" | "credit";
  amountMinor: number;
}

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
  /** The cashier and their open shift are not sent: Rust derives both from
   * the signed-in session (`billing_create_sale`). */
  tableId: number | null;
  tenderedMinor?: number | null;
  changeMinor?: number | null;
  /** The tender breakdown; omit for a single-method sale. Must add up to
   * the server's total. */
  payments?: PaymentPart[];
  customerId?: number | null;
  serviceChargeMinor?: number;
  /** Required when any part is `credit`. */
  dueDate?: string | null;
}

export interface SaleLine {
  itemId: number;
  itemName: string;
  qty: number;
  /** Display unit ("kg"), for a `sold_by_amount` line — `null` for a
   * normal per-piece item. */
  unit: string | null;
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
  paymentMethod: SalePaymentMethod;
  tenderedMinor?: number | null;
  changeMinor?: number | null;
  cashierId: number | null;
  cashierName: string | null;
  tableId: number | null;
  tableName: string | null;
  shiftId: number | null;
  orderId?: number | null;
  orderNo?: number | null;
  createdAt: string;
  items: SaleLine[];
  orderType?: string | null;
  customerName?: string | null;
  customerPhone?: string | null;
  deliveryAddress?: string | null;
  deliveryRiderName?: string | null;
  deliveryRiderPhone?: string | null;
  deliveryPlatformName?: string | null;
  platformOrderRef?: string | null;
  deliveryChargeMinor?: number | null;
  serviceChargeMinor: number;
  payments: PaymentPart[];
  customerId: number | null;
  customerEmail: string | null;
  dueDate: string | null;
}

/** One row of the "recent sales" list the refund flow searches. */
export interface SaleListItem {
  id: number;
  totalMinor: number;
  paymentMethod: SalePaymentMethod;
  cashierName: string | null;
  createdAt: string;
}

// TableSummary / ParkedCartLine / ParkedOrder now live in ./tables — this
// module only needs `tableId` on CreateSaleInput/Sale above.
