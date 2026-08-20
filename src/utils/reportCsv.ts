import { buildCsv, downloadTextFile } from "./csv";
import { minorToDecimal } from "./format";
import type { AppConfig, DailySales, SalesSummary, TableSalesSummary, TopItem } from "../types";

export interface ReportData {
  summary: SalesSummary;
  topItems: TopItem[];
  series: DailySales[];
  config: AppConfig;
}

/** CSV with three sections — summary, top items, and the daily series —
 * separated by a blank line so it opens cleanly in a spreadsheet. */
export function buildReportCsv({ summary, topItems, series }: ReportData): string {
  const sections: string[] = [];

  sections.push(
    buildCsv([
      ["Report range", `${summary.startDate} to ${summary.endDate}`],
      ["Total sales", minorToDecimal(summary.totalSalesMinor).toFixed(2)],
      ["Transactions", summary.transactionCount],
      ["Average sale", minorToDecimal(summary.averageSaleMinor).toFixed(2)],
    ]),
  );

  sections.push(
    buildCsv([
      ["Top-selling items"],
      ["Item", "Qty sold", "Revenue"],
      ...topItems.map((item) => [item.itemName, item.qtySold, minorToDecimal(item.revenueMinor).toFixed(2)]),
    ]),
  );

  sections.push(
    buildCsv([
      ["Sales by day"],
      ["Date", "Total", "Transactions"],
      ...series.map((point) => [point.date, minorToDecimal(point.totalMinor).toFixed(2), point.transactionCount]),
    ]),
  );

  return sections.join("\r\n\r\n");
}

export function downloadReportCsv(data: ReportData): void {
  downloadTextFile(
    buildReportCsv(data),
    `sales-report-${data.summary.startDate}-to-${data.summary.endDate}.csv`,
    "text/csv;charset=utf-8",
  );
}

/** CSV for the Table Wise Sales report — one row per table/counter, plus a
 * grand total row so the file reconciles the same way the on-screen table
 * and PDF do. */
export function buildTableSalesCsv(report: TableSalesSummary): string {
  return buildCsv([
    ["Report range", `${report.startDate} to ${report.endDate}`],
    [],
    ["Table / Counter", "Transactions", "Amount"],
    ...report.rows.map((row) => [row.label, row.transactionCount, minorToDecimal(row.totalMinor).toFixed(2)]),
    [],
    ["Grand Total", "", minorToDecimal(report.grandTotalMinor).toFixed(2)],
  ]);
}

export function downloadTableSalesCsv(report: TableSalesSummary): void {
  downloadTextFile(
    buildTableSalesCsv(report),
    `table-wise-sales-${report.startDate}-to-${report.endDate}.csv`,
    "text/csv;charset=utf-8",
  );
}
