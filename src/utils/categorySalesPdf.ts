import type { jsPDF } from "jspdf";
import { formatMinor, formatQty } from "./format";
import { downloadPdf } from "./pdfExport";
import {
  drawBoldRow,
  drawFooter,
  drawGrandTotalRow,
  drawHeader,
  drawSectionHeader,
  drawTable,
  estimateReceiptHeightMm,
  newReceiptDoc,
} from "./printLayout";
import type { AppConfig, CategorySalesReport } from "../types";

/** Builds the "Category Wise Sale" report PDF: one section per category
 * (header, item rows, subtotal), grand total at the end — the PDF-fallback
 * twin of `printer::escpos::build_category_sales_bytes`. */
export function buildCategorySalesPdf(report: CategorySalesReport, config: AppConfig): jsPDF {
  const itemRowCount = report.groups.reduce((sum, g) => sum + g.items.length, 0);
  const estimatedHeightMm =
    estimateReceiptHeightMm(report.groups.length * 12 + itemRowCount * 6 + 20, config.receiptFooter);
  // A category report can run much longer than a receipt (many categories,
  // many items each) — `newReceiptDoc`'s name is receipt-specific, but it's
  // just an 80mm-wide-doc-with-a-given-height constructor, equally correct
  // to reuse here.
  const doc = newReceiptDoc(estimatedHeightMm);

  let y = drawHeader(doc, config.businessName, ["Category Wise Sale", `${report.startDate} to ${report.endDate}`]);

  for (const group of report.groups) {
    y = drawSectionHeader(doc, y, group.categoryName);
    y = drawTable(
      doc,
      y,
      ["Item", "Qty", "Amount"],
      group.items.map((line) => [line.itemName, formatQty(line.qtySold), formatMinor(line.revenueMinor, config.currency)]),
    );

    y = drawBoldRow(doc, y, "Subtotal", formatMinor(group.subtotalMinor, config.currency));
  }

  y = drawGrandTotalRow(doc, y, "GRAND TOTAL", formatMinor(report.grandTotalMinor, config.currency), "heavy");

  drawFooter(doc, y, config.receiptFooter);
  return doc;
}

export function downloadCategorySalesPdf(report: CategorySalesReport, config: AppConfig): Promise<boolean> {
  return downloadPdf(buildCategorySalesPdf(report, config), `category-wise-sale-${report.startDate}-to-${report.endDate}.pdf`);
}
