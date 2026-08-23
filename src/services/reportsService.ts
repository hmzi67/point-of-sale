import type {
  CategorySalesReport,
  DailySales,
  FullReport,
  ProductSalesSummaryReport,
  SalesSummary,
  TableSalesSummary,
  TopItem,
  TopItemSort,
} from "../types";
import { PLATFORM } from "../types";
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

/** One row per table plus a "Counter / Takeaway" row, summing to the same
 * gross total `getSalesSummary` reports for the same range — the Table
 * Wise Sales report. Only meaningful when the `tables` module is enabled;
 * the caller is responsible for not offering this view otherwise. */
export function getTableSalesSummary(startDate: string, endDate: string): Promise<TableSalesSummary> {
  return call<TableSalesSummary>("reports_get_table_sales_summary", { startDate, endDate });
}

/** Prints the Table Wise Sales report on a USB thermal printer — same
 * auto-detect/fall-back-to-PDF contract as the other print buttons. */
export function printTableSalesThermal(startDate: string, endDate: string): Promise<void> {
  return call<void>("reports_print_table_sales_summary", { startDate, endDate });
}

/** The "Product Wise Sales" report: every item sold in the range (ranked by
 * `sortBy`), optionally narrowed to one category, plus a "no sales this
 * period" list of active items that sold zero units. */
export function getProductSalesSummary(
  startDate: string,
  endDate: string,
  categoryId: number | null,
  sortBy: TopItemSort,
): Promise<ProductSalesSummaryReport> {
  return call<ProductSalesSummaryReport>("reports_get_product_sales_summary", {
    startDate,
    endDate,
    categoryId: categoryId ?? undefined,
    sortBy,
  });
}

/** The "Generate Full Report" consolidated document: Overview (incl. net
 * profit), Category Wise Sale, Product Wise Sales, and Table Wise Sales
 * (only when `tables` is enabled) for one date range, all in one payload —
 * source for both the combined PDF download and the thermal print below. */
export function getFullReport(startDate: string, endDate: string): Promise<FullReport> {
  return call<FullReport>("reports_get_full_report", { startDate, endDate, platform: PLATFORM });
}

/** Prints the same consolidated Full Report on a USB thermal printer. */
export function printFullReportThermal(startDate: string, endDate: string): Promise<void> {
  return call<void>("reports_print_full_report", { startDate, endDate, platform: PLATFORM });
}
