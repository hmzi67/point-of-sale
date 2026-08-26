import { jsPDF } from "jspdf";
import autoTable from "jspdf-autotable";
import { formatMinor, formatQty } from "./format";
import { downloadPdf } from "./pdfExport";
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
  const doc = new jsPDF({ unit: "mm", format: "a4" });
  const pageWidth = doc.internal.pageSize.getWidth();
  const margin = 14;
  const money = (minor: number) => formatMinor(minor, config.currency);
  let y = 18;

  doc.setFont("helvetica", "bold");
  doc.setFontSize(16);
  doc.text(`${config.businessName} — Full Sales Report`, margin, y);
  y += 7;

  doc.setFont("helvetica", "normal");
  doc.setFontSize(10);
  doc.setTextColor(100);
  doc.text(`${report.startDate} to ${report.endDate}`, margin, y);
  y += 10;
  doc.setTextColor(0);

  // --- Overview -------------------------------------------------------
  doc.setFont("helvetica", "bold");
  doc.setFontSize(12);
  doc.text("Overview", margin, y);
  y += 6;

  const { overview } = report;
  const stats: Array<[string, string]> = [
    ["Total sales", money(overview.totalSalesMinor)],
    ["Refunds", money(overview.refundsMinor)],
    ["Transactions", String(overview.transactionCount)],
    ["Average sale", money(report.averageSaleMinor)],
    ["Net profit", money(overview.netProfitMinor)],
  ];
  const columnWidth = (pageWidth - margin * 2) / stats.length;
  stats.forEach(([label, value], index) => {
    const x = margin + index * columnWidth;
    doc.setFontSize(9);
    doc.setTextColor(100);
    doc.text(label.toUpperCase(), x, y);
    doc.setFontSize(13);
    doc.setFont("helvetica", "bold");
    doc.setTextColor(0);
    doc.text(value, x, y + 7);
    doc.setFont("helvetica", "normal");
  });
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
  doc.setFont("helvetica", "bold");
  doc.setFontSize(12);
  doc.text("Refunds", margin, y);
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
      styles: { fontSize: 8, cellPadding: 1.8 },
      headStyles: { fillColor: [30, 41, 59] },
      columnStyles: { 0: { cellWidth: 14 }, 5: { halign: "right" } },
      theme: "striped",
      foot: [["", "", "", "", "Total Refunded", money(report.refunds.grandTotalRefundedMinor)]],
      footStyles: { fillColor: [241, 245, 249], textColor: [15, 23, 42], fontStyle: "bold" },
    });
    y = (doc as unknown as { lastAutoTable: { finalY: number } }).lastAutoTable.finalY + 10;
  }

  // --- Category Wise Sale ----------------------------------------------
  if (y > 250) {
    doc.addPage();
    y = 18;
  }
  doc.setFont("helvetica", "bold");
  doc.setFontSize(12);
  doc.text("Category Wise Sale", margin, y);
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
      styles: { fontSize: 8, cellPadding: 1.8 },
      headStyles: { fillColor: [30, 41, 59] },
      columnStyles: { 1: { halign: "right" }, 2: { halign: "right" } },
      theme: "grid",
      foot: [["Subtotal", "", money(group.subtotalMinor)]],
      footStyles: { fillColor: [241, 245, 249], textColor: [15, 23, 42], fontStyle: "bold" },
    });
    y = (doc as unknown as { lastAutoTable: { finalY: number } }).lastAutoTable.finalY + 4;
  }
  doc.setFont("helvetica", "bold");
  doc.setFontSize(10);
  doc.text("Grand Total", margin, y);
  doc.text(money(report.categorySales.grandTotalMinor), pageWidth - margin, y, { align: "right" });
  doc.setFont("helvetica", "normal");
  y += 10;

  // --- Product Wise Sales ------------------------------------------------
  if (y > 250) {
    doc.addPage();
    y = 18;
  }
  doc.setFont("helvetica", "bold");
  doc.setFontSize(12);
  doc.text("Product Wise Sales", margin, y);
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
    styles: { fontSize: 8, cellPadding: 1.8 },
    headStyles: { fillColor: [30, 41, 59] },
    columnStyles: { 0: { cellWidth: 10 }, 3: { halign: "right" }, 4: { halign: "right" } },
    theme: "striped",
  });
  y = (doc as unknown as { lastAutoTable: { finalY: number } }).lastAutoTable.finalY + 10;

  // --- Table Wise Sales (only when the `tables` module is enabled) ------
  if (report.tableSales) {
    if (y > 250) {
      doc.addPage();
      y = 18;
    }
    doc.setFont("helvetica", "bold");
    doc.setFontSize(12);
    doc.text("Table Wise Sales", margin, y);
    y += 2;
    autoTable(doc, {
      startY: y,
      margin: { left: margin, right: margin },
      head: [["Table / Counter", "Txns", "Amount"]],
      body: report.tableSales.rows.map((row) => [row.label, String(row.transactionCount), money(row.totalMinor)]),
      styles: { fontSize: 8, cellPadding: 1.8 },
      headStyles: { fillColor: [30, 41, 59] },
      columnStyles: { 1: { halign: "right" }, 2: { halign: "right" } },
      theme: "grid",
      foot: [["Grand Total", "", money(report.tableSales.grandTotalMinor)]],
      footStyles: { fillColor: [241, 245, 249], textColor: [15, 23, 42], fontStyle: "bold" },
    });
  }

  return doc;
}

export function downloadFullReportPdf(report: FullReport, config: AppConfig): Promise<boolean> {
  return downloadPdf(buildFullReportPdf(report, config), `full-report-${report.startDate}-to-${report.endDate}.pdf`);
}
