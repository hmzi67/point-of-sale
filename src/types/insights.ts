/** Profit, payment, tax, void and inventory reports — see `db/insights.rs`. */

export type PeriodGrouping = "day" | "week" | "month";

export interface PeriodRow {
  period: string;
  transactions: number;
  netRevenueMinor: number;
  cogsMinor: number;
  grossProfitMinor: number;
  expensesMinor: number;
  netProfitMinor: number;
}

export interface ProfitLoss {
  startDate: string;
  endDate: string;
  grossSalesMinor: number;
  discountsMinor: number;
  refundsMinor: number;
  serviceChargeMinor: number;
  netRevenueMinor: number;
  taxMinor: number;
  cogsMinor: number;
  grossProfitMinor: number;
  /** `null` when the module is off (not tracked). */
  expensesMinor: number | null;
  salariesMinor: number | null;
  netProfitMinor: number;
  transactions: number;
  periods: PeriodRow[];
}

export interface OrderProfitRow {
  saleId: number;
  orderNo: number | null;
  createdAt: string;
  cashierName: string | null;
  customerName: string | null;
  paymentMethod: string;
  itemCount: number;
  revenueMinor: number;
  costMinor: number;
  refundedMinor: number;
  profitMinor: number;
  marginPercent: number;
}

export interface PaymentTypeRow {
  method: string;
  amountMinor: number;
  saleCount: number;
}

export interface PaymentTypeReport {
  rows: PaymentTypeRow[];
  collections: PaymentTypeRow[];
  refundsMinor: number;
  splitSaleCount: number;
}

export interface TaxDayRow {
  date: string;
  transactions: number;
  taxableMinor: number;
  taxMinor: number;
  serviceChargeMinor: number;
  refundsMinor: number;
}

export interface TaxReport {
  rows: TaxDayRow[];
  taxableMinor: number;
  taxMinor: number;
  serviceChargeMinor: number;
  refundsMinor: number;
  taxExemptSaleCount: number;
}

export interface VoidRow {
  orderId: number;
  orderNo: number;
  itemName: string;
  qty: number;
  valueMinor: number;
  reason: string | null;
  voidedBy: string | null;
  voidedAt: string;
}

export interface CancelledOrderRow {
  orderId: number;
  orderNo: number;
  orderType: string;
  reason: string | null;
  cancelledAt: string;
  valueMinor: number;
}

export interface VoidReport {
  voids: VoidRow[];
  cancelledOrders: CancelledOrderRow[];
  voidValueMinor: number;
  cancelledValueMinor: number;
  refundsMinor: number;
  refundCount: number;
}

export interface StockLevelRow {
  itemId: number;
  itemName: string;
  categoryName: string | null;
  unit: string | null;
  stockQty: number;
  lowStockThreshold: number;
  costMinor: number;
  priceMinor: number;
  costValueMinor: number;
  retailValueMinor: number;
  isLowStock: boolean;
  incomingQty: number;
}

export interface StockLevelReport {
  rows: StockLevelRow[];
  costValueMinor: number;
  retailValueMinor: number;
  lowStockCount: number;
  outOfStockCount: number;
}

export interface UsageRow {
  itemId: number;
  itemName: string;
  unit: string | null;
  openingQty: number;
  receivedQty: number;
  soldQty: number;
  returnedQty: number;
  damagedQty: number;
  adjustedQty: number;
  closingQty: number;
  turnover: number;
  daysOfCover: number | null;
  cogsMinor: number;
}

export interface ExpiryRow {
  itemId: number;
  itemName: string;
  batchNo: string | null;
  expiryDate: string;
  /** Negative once expired. */
  daysLeft: number;
  qtyReceived: number;
  stockQty: number;
  supplierName: string;
  grnId: number;
  receivedDate: string;
}
