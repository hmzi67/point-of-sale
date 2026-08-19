/** Money fields cross IPC as integer minor units, matching the Rust side. */

export interface SalesSummary {
  startDate: string;
  endDate: string;
  totalSalesMinor: number;
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
  transactionCount: number;
}

export type DateRangePreset = "today" | "thisWeek" | "thisMonth" | "custom";

export interface DateRange {
  startDate: string;
  endDate: string;
}
