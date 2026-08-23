import type { jsPDF } from "jspdf";
import { formatMinor } from "./format";
import { downloadPdf } from "./pdfExport";
import { drawFooter, drawHeader, drawTable, drawTotalsBlock, newReceiptDoc } from "./printLayout";
import type { AppConfig, TableSalesSummary } from "../types";

/** Builds the Table Wise Sales report PDF: one row per table (plus
 * "Counter / Takeaway"), grand total at the end. */
export function buildTableSalesPdf(report: TableSalesSummary, config: AppConfig): jsPDF {
  const estimatedHeightMm = 46 + report.rows.length * 6 + 20 + (config.receiptFooter.trim() ? 10 : 0);
  const doc = newReceiptDoc(estimatedHeightMm);

  let y = drawHeader(doc, config.businessName, ["Table Wise Sales", `${report.startDate} to ${report.endDate}`]);

  y = drawTable(
    doc,
    y,
    ["Table / Counter", "Txns", "Amount"],
    report.rows.map((row) => [row.label, String(row.transactionCount), formatMinor(row.totalMinor, config.currency)]),
  );

  y = drawTotalsBlock(doc, y, [], "Grand Total", formatMinor(report.grandTotalMinor, config.currency));

  drawFooter(doc, y, config.receiptFooter);
  return doc;
}

export function downloadTableSalesPdf(report: TableSalesSummary, config: AppConfig): Promise<boolean> {
  return downloadPdf(buildTableSalesPdf(report, config), `table-wise-sales-${report.startDate}-to-${report.endDate}.pdf`);
}
