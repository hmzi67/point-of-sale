import type { jsPDF } from "jspdf";
import { formatMinor } from "./format";
import { downloadPdf } from "./pdfExport";
import { drawFooter, drawHeader, drawSectionHeader, drawTable, newReceiptDoc, PAGE_WIDTH_MM } from "./printLayout";
import type { AppConfig, CategorySalesReport } from "../types";

/** Builds the "Category Wise Sale" report PDF: one section per category
 * (header, item rows, subtotal), grand total at the end — the PDF-fallback
 * twin of `printer::escpos::build_category_sales_bytes`. */
export function buildCategorySalesPdf(report: CategorySalesReport, config: AppConfig): jsPDF {
  const itemRowCount = report.groups.reduce((sum, g) => sum + g.items.length, 0);
  const estimatedHeightMm =
    46 + report.groups.length * 12 + itemRowCount * 6 + 20 + (config.receiptFooter.trim() ? 10 : 0);
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
      group.items.map((line) => [line.itemName, String(line.qtySold), formatMinor(line.revenueMinor, config.currency)]),
    );

    doc.setFont("helvetica", "bold");
    doc.setFontSize(9);
    doc.text("Subtotal", 4, y);
    doc.text(formatMinor(group.subtotalMinor, config.currency), PAGE_WIDTH_MM - 4, y, { align: "right" });
    doc.setFont("helvetica", "normal");
    y += 8;
  }

  doc.setDrawColor(15, 23, 42);
  doc.setLineWidth(0.4);
  doc.line(4, y, PAGE_WIDTH_MM - 4, y);
  y += 6;
  doc.setFont("helvetica", "bold");
  doc.setFontSize(11);
  doc.text("GRAND TOTAL", 4, y);
  doc.text(formatMinor(report.grandTotalMinor, config.currency), PAGE_WIDTH_MM - 4, y, { align: "right" });
  doc.setFont("helvetica", "normal");
  y += 6;

  drawFooter(doc, y, config.receiptFooter);
  return doc;
}

export function downloadCategorySalesPdf(report: CategorySalesReport, config: AppConfig): Promise<boolean> {
  return downloadPdf(buildCategorySalesPdf(report, config), `category-wise-sale-${report.startDate}-to-${report.endDate}.pdf`);
}
