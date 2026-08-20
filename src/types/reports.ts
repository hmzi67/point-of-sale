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

export type DateRangePreset = "today" | "thisWeek" | "thisMonth" | "custom";

export interface DateRange {
  startDate: string;
  endDate: string;
}
