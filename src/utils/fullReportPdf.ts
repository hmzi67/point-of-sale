import type { jsPDF } from "jspdf";
import autoTable from "jspdf-autotable";
import {
  A4_MARGIN_MM,
  A4_TABLE_STYLES,
  drawA4SectionTitle,
  drawA4StatTiles,
  drawA4Title,
  ensureSectionRoom,
  newA4Doc,
} from "./a4Pdf";
import { formatMinor, formatQty } from "./format";
import { downloadPdf } from "./pdfExport";
import { lastTableEndY } from "./printLayout";
import type { AppConfig, FullReport } from "../types";

/** Builds the "Generate Full Report" consolidated PDF — Overview, Category
 * Wise Sale, Product Wise Sales, and (if enabled) Table Wise Sales for one
 * date range, all in one A4 document. Full-width A4 (not the 80mm
 * receipt-shaped page the other report/receipt PDFs use — see
 * `printLayout.ts`) since this is meant to read as one real "everything for
 * this period" report, not a thermal-paper-shaped slip — the ESC/POS twin
 * (`printer::escpos::build_full_report_bytes`) is what covers the thermal
 * paper case. Kept in its own module, pulled in only via dynamic import,
 * same deferral every other jsPDF builder here uses. */
export function buildFullReportPdf(report: FullReport, config: AppConfig): jsPDF {
  const doc = newA4Doc();
  const pageWidth = doc.internal.pageSize.getWidth();
  const margin = A4_MARGIN_MM;
  const money = (minor: number) => formatMinor(minor, config.currency);
  let y = drawA4Title(doc, `${config.businessName} — Full Sales Report`, `${report.startDate} to ${report.endDate}`);

  // --- Overview -------------------------------------------------------
  drawA4SectionTitle(doc, "Overview", y);
  y += 6;

  const { overview } = report;
  drawA4StatTiles(
    doc,
    y,
    [
      ["Total sales", money(overview.totalSalesMinor)],
      ["Refunds", money(overview.refundsMinor)],
      ["Transactions", String(overview.transactionCount)],
      ["Average sale", money(report.averageSaleMinor)],
      ["Net profit", money(overview.netProfitMinor)],
    ],
    13,
  );
  y += 14;

  if (overview.totalExpensesMinor != null || overview.totalSalaryPaidMinor != null) {
    doc.setFontSize(9);
    doc.setTextColor(100);
    const parts: string[] = [];
    if (overview.totalExpensesMinor != null) parts.push(`Expenses: ${money(overview.totalExpensesMinor)}`);
    if (overview.totalSalaryPaidMinor != null) parts.push(`Salary paid: ${money(overview.totalSalaryPaidMinor)}`);
    doc.text(parts.join("   ·   "), margin, y);
    doc.setTextColor(0);
    y += 8;
  }

  // --- Refunds ------------------------------------------------------------
  drawA4SectionTitle(doc, "Refunds", y);
  y += 2;

  if (report.refunds.refunds.length === 0) {
    doc.setFont("helvetica", "normal");
    doc.setFontSize(9);
    doc.setTextColor(120);
    doc.text("No refunds in this period", margin, y + 4);
    doc.setTextColor(0);
    y += 10;
  } else {
    autoTable(doc, {
      startY: y,
      margin: { left: margin, right: margin },
      head: [["Vno", "Item(s)", "Reason", "By", "Date", "Amount"]],
      body: report.refunds.refunds.map((refund) => [
        String(refund.originalSaleId),
        refund.items.map((line) => `${line.itemName} x${line.qtyRefunded}`).join(", "),
        refund.reason ?? "—",
        refund.refundedByName ?? "—",
        refund.createdAt,
        money(refund.totalRefundAmountMinor),
      ]),
      styles: A4_TABLE_STYLES.styles,
      headStyles: A4_TABLE_STYLES.headStyles,
      columnStyles: { 0: { cellWidth: 14 }, 5: { halign: "right" } },
      theme: "striped",
      foot: [["", "", "", "", "Total Refunded", money(report.refunds.grandTotalRefundedMinor)]],
      footStyles: A4_TABLE_STYLES.footStyles,
    });
    y = lastTableEndY(doc, y) + 10;
  }

  // --- Category Wise Sale ----------------------------------------------
  y = ensureSectionRoom(doc, y);
  drawA4SectionTitle(doc, "Category Wise Sale", y);
  y += 2;

  for (const group of report.categorySales.groups) {
    y += 4;
    doc.setFontSize(10);
    doc.setFont("helvetica", "bold");
    doc.text(group.categoryName, margin, y);
    y += 2;
    autoTable(doc, {
      startY: y,
      margin: { left: margin, right: margin },
      head: [["Item", "Qty", "Revenue"]],
      body: group.items.map((line) => [line.itemName, formatQty(line.qtySold), money(line.revenueMinor)]),
      styles: A4_TABLE_STYLES.styles,
      headStyles: A4_TABLE_STYLES.headStyles,
      columnStyles: { 1: { halign: "right" }, 2: { halign: "right" } },
      theme: "grid",
      foot: [["Subtotal", "", money(group.subtotalMinor)]],
      footStyles: A4_TABLE_STYLES.footStyles,
    });
    y = lastTableEndY(doc, y) + 4;
  }
  doc.setFont("helvetica", "bold");
  doc.setFontSize(10);
  doc.text("Grand Total", margin, y);
  doc.text(money(report.categorySales.grandTotalMinor), pageWidth - margin, y, { align: "right" });
  doc.setFont("helvetica", "normal");
  y += 10;

  // --- Product Wise Sales ------------------------------------------------
  y = ensureSectionRoom(doc, y);
  drawA4SectionTitle(doc, "Product Wise Sales", y);
  y += 2;
  autoTable(doc, {
    startY: y,
    margin: { left: margin, right: margin },
    head: [["#", "Item", "Category", "Qty sold", "Revenue"]],
    body: report.productSales.rows.map((row) => [
      String(row.rank),
      row.itemName,
      row.categoryName,
      formatQty(row.qtySold),
      money(row.revenueMinor),
    ]),
    styles: A4_TABLE_STYLES.styles,
    headStyles: A4_TABLE_STYLES.headStyles,
    columnStyles: { 0: { cellWidth: 10 }, 3: { halign: "right" }, 4: { halign: "right" } },
    theme: "striped",
  });
  y = lastTableEndY(doc, y) + 10;

  // --- Table Wise Sales (only when the `tables` module is enabled) ------
  if (report.tableSales) {
    y = ensureSectionRoom(doc, y);
    drawA4SectionTitle(doc, "Table Wise Sales", y);
    y += 2;
    autoTable(doc, {
      startY: y,
      margin: { left: margin, right: margin },
      head: [["Table / Counter", "Txns", "Amount"]],
      body: report.tableSales.rows.map((row) => [row.label, String(row.transactionCount), money(row.totalMinor)]),
      styles: A4_TABLE_STYLES.styles,
      headStyles: A4_TABLE_STYLES.headStyles,
      columnStyles: { 1: { halign: "right" }, 2: { halign: "right" } },
      theme: "grid",
      foot: [["Grand Total", "", money(report.tableSales.grandTotalMinor)]],
      footStyles: A4_TABLE_STYLES.footStyles,
    });
  }

  return doc;
}

export function downloadFullReportPdf(report: FullReport, config: AppConfig): Promise<boolean> {
  return downloadPdf(buildFullReportPdf(report, config), `full-report-${report.startDate}-to-${report.endDate}.pdf`);
}
