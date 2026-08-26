import { buildCsv, downloadTextFile } from "./csv";
import { minorToDecimal } from "./format";
import type { AppConfig, DailySales, SalesSummary, TopItem } from "../types";

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
      // Rounded to 2dp, same as everywhere else a fractional (`soldByAmount`)
      // qty is displayed — a raw sum straight from the database can carry
      // floating-point noise (e.g. 12.499999999999998).
      ...topItems.map((item) => [item.itemName, Math.round(item.qtySold * 100) / 100, minorToDecimal(item.revenueMinor).toFixed(2)]),
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

export function downloadReportCsv(data: ReportData): Promise<boolean> {
  return downloadTextFile(buildReportCsv(data), `sales-report-${data.summary.startDate}-to-${data.summary.endDate}.csv`);
}
