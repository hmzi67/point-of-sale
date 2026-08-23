/** Money fields cross IPC as integer minor units, matching the Rust side. */

export interface SalesSummary {
  startDate: string;
  endDate: string;
  /** Gross — unaffected by any refund. */
  totalSalesMinor: number;
  /** Refunds recorded in the range (by refund date, not original sale date). */
  refundsMinor: number;
  /** `totalSalesMinor - refundsMinor` — what profit math should use. */
  netSalesMinor: number;
  transactionCount: number;
  averageSaleMinor: number;
}

export type TopItemSort = "quantity" | "revenue";

export interface TopItem {
  itemId: number;
  itemName: string;
  qtySold: number;
  revenueMinor: number;
}

export interface DailySales {
  /** `YYYY-MM-DD`. */
  date: string;
  totalMinor: number;
  refundsMinor: number;
  netMinor: number;
  transactionCount: number;
}

export interface CategorySalesLine {
  itemId: number;
  itemName: string;
  qtySold: number;
  revenueMinor: number;
}

export interface CategorySalesGroup {
  categoryId: number | null;
  categoryName: string;
  items: CategorySalesLine[];
  subtotalMinor: number;
}

export interface CategorySalesReport {
  startDate: string;
  endDate: string;
  groups: CategorySalesGroup[];
  grandTotalMinor: number;
}

export interface TableSalesRow {
  /** `null` for the "Counter / Takeaway" row. */
  tableId: number | null;
  label: string;
  totalMinor: number;
  transactionCount: number;
}

export interface TableSalesSummary {
  startDate: string;
  endDate: string;
  /** Highest-total first. */
  rows: TableSalesRow[];
  grandTotalMinor: number;
}

export interface ProductSalesRow {
  itemId: number;
  itemName: string;
  categoryId: number | null;
  categoryName: string;
  qtySold: number;
  revenueMinor: number;
  /** 1-based position under the report's `sortBy`. */
  rank: number;
}

export interface ProductSalesNoSaleRow {
  itemId: number;
  itemName: string;
  categoryId: number | null;
  categoryName: string;
}

export interface ProductSalesSummaryReport {
  startDate: string;
  endDate: string;
  sortBy: TopItemSort;
  /** Every item with at least one sale in the range, ranked. */
  rows: ProductSalesRow[];
  /** Active items with zero sales in the range — shown as their own
   * section so slow-moving stock stays visible, not hidden. */
  noSalesItems: ProductSalesNoSaleRow[];
}

export type DateRangePreset = "today" | "thisWeek" | "thisMonth" | "custom";

export interface DateRange {
  startDate: string;
  endDate: string;
}
