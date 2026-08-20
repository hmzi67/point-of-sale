/** Money fields cross IPC as integer minor units, matching the Rust side. */

export interface RefundableLine {
  saleItemId: number;
  itemId: number;
  itemName: string;
  qty: number;
  priceAtSaleMinor: number;
  qtyAlreadyRefunded: number;
  /** `qty - qtyAlreadyRefunded` — the UI's upper bound on this line. */
  qtyRefundable: number;
}

export interface RefundableSale {
  saleId: number;
  createdAt: string;
  totalMinor: number;
  paymentMethod: string;
  items: RefundableLine[];
}

export interface RefundLineInput {
  saleItemId: number;
  qty: number;
  amountMinor: number;
}

export interface RefundLine {
  saleItemId: number;
  itemId: number;
  itemName: string;
  qtyRefunded: number;
  amountRefundedMinor: number;
}

export interface Refund {
  id: number;
  originalSaleId: number;
  refundedBy: number | null;
  refundedByName: string | null;
  reason: string | null;
  totalRefundAmountMinor: number;
  createdAt: string;
  items: RefundLine[];
}
