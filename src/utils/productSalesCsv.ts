import { buildCsv, downloadTextFile } from "./csv";
import { minorToDecimal } from "./format";
import type { ProductSalesSummaryReport } from "../types";

/** CSV for the Product Wise Sales report — the ranked "sold" rows first
 * (matching the on-screen sort), then a separate "no sales this period"
 * section so slow-moving stock is exported too, not just hidden. */
export function buildProductSalesCsv(report: ProductSalesSummaryReport): string {
  return buildCsv([
    ["Report range", `${report.startDate} to ${report.endDate}`],
    ["Sorted by", report.sortBy === "revenue" ? "Revenue" : "Quantity"],
    [],
    ["Rank", "Item", "Category", "Qty sold", "Revenue"],
    ...report.rows.map((row) => [
      row.rank,
      row.itemName,
      row.categoryName,
      row.qtySold,
      minorToDecimal(row.revenueMinor).toFixed(2),
    ]),
    [],
    ["No sales this period"],
    ["Item", "Category"],
    ...report.noSalesItems.map((row) => [row.itemName, row.categoryName]),
  ]);
}

export function downloadProductSalesCsv(report: ProductSalesSummaryReport): void {
  downloadTextFile(
    buildProductSalesCsv(report),
    `product-wise-sales-${report.startDate}-to-${report.endDate}.csv`,
    "text/csv;charset=utf-8",
  );
}
