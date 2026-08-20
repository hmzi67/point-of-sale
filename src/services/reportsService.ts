import type { CategorySalesReport, DailySales, SalesSummary, TopItem, TopItemSort } from "../types";
import { call } from "./tauriClient";

/** `startDate`/`endDate` are `YYYY-MM-DD`, inclusive. */
export function getSalesSummary(startDate: string, endDate: string): Promise<SalesSummary> {
  return call<SalesSummary>("reports_get_sales_summary", { startDate, endDate });
}

export function getTopItems(
  startDate: string,
  endDate: string,
  limit: number,
  sortBy: TopItemSort,
): Promise<TopItem[]> {
  return call<TopItem[]>("reports_get_top_items", { startDate, endDate, limit, sortBy });
}

/** One row per calendar day in the range, zero-filled where there were no sales. */
export function getSalesOverTime(startDate: string, endDate: string): Promise<DailySales[]> {
  return call<DailySales[]>("reports_get_sales_over_time", { startDate, endDate });
}

/** Every item sold in the range, grouped by category with a subtotal per
 * category and a grand total — the "Category Wise Sale" report. */
export function getCategorySales(startDate: string, endDate: string): Promise<CategorySalesReport> {
  return call<CategorySalesReport>("reports_get_category_sales", { startDate, endDate });
}

/** Prints the Category Wise Sale report on a USB thermal printer — same
 * auto-detect/fall-back-to-PDF contract as the billing receipt. */
export function printCategorySalesThermal(startDate: string, endDate: string): Promise<void> {
  return call<void>("reports_print_category_sales", { startDate, endDate });
}
