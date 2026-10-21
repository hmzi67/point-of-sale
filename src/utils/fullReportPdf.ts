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
import { formatMinor } from "./format";
import { downloadPdf } from "./pdfExport";
import { lastTableEndY } from "./printLayout";
import type { AppConfig, FullReport } from "../types";

/** Builds the Overall Sales Report consolidated PDF — Key Sales KPIs,
 * Financial Breakdown, Payment Methods Summary, and Refunds for the
 * selected period, all in one clean A4 document. Category-wise and
 * product-wise lists are omitted to keep the report focused on executive
 * totals and overall store performance. */
export function buildFullReportPdf(report: FullReport, config: AppConfig): jsPDF {
  const doc = newA4Doc();
  const margin = A4_MARGIN_MM;
  const money = (minor: number) => formatMinor(minor, config.currency);
  const subtitle =
    report.startDate === report.endDate
      ? `Daily Sales Report · ${report.startDate}`
      : `Reporting Period · ${report.startDate} to ${report.endDate}`;
  let y = drawA4Title(doc, `${config.businessName} — Overall Sales Report`, subtitle);

  // --- Executive KPI Stat Tiles ----------------------------------------
  drawA4SectionTitle(doc, "Sales Performance Overview", y);
  y += 6;

  const { overview } = report;
  const netSalesMinor = report.salesMetrics?.netSalesMinor ?? overview.totalSalesMinor - overview.refundsMinor;

  drawA4StatTiles(
    doc,
    y,
    [
      ["Total Orders", String(overview.transactionCount)],
      ["Gross Sales", money(overview.totalSalesMinor)],
      ["Net Sales", money(netSalesMinor)],
      ["Average Order", money(report.averageSaleMinor)],
      ["Net Profit", money(overview.netProfitMinor)],
    ],
    12,
  );
  y += 16;

  // --- Overall Financial Summary Table ---------------------------------
  drawA4SectionTitle(doc, "Sales & Financial Breakdown", y);
  y += 4;

  const financialRows: Array<[string, string]> = [["Gross Sales", money(overview.totalSalesMinor)]];

  if (report.salesMetrics?.totalDiscountMinor && report.salesMetrics.totalDiscountMinor > 0) {
    financialRows.push(["Total Discounts", `-${money(report.salesMetrics.totalDiscountMinor)}`]);
  }
  if (report.salesMetrics?.totalTaxMinor && report.salesMetrics.totalTaxMinor > 0) {
    financialRows.push(["Total Tax Collected", money(report.salesMetrics.totalTaxMinor)]);
  }
  financialRows.push(["Refunds Issued", `-${money(overview.refundsMinor)}`]);
  financialRows.push(["Net Sales", money(netSalesMinor)]);

  if (overview.totalExpensesMinor != null) {
    financialRows.push(["Operating Expenses", `-${money(overview.totalExpensesMinor)}`]);
  }
  if (overview.totalSalaryPaidMinor != null) {
    financialRows.push(["Salary Paid", `-${money(overview.totalSalaryPaidMinor)}`]);
  }

  autoTable(doc, {
    startY: y,
    margin: { left: margin, right: margin },
    head: [["Metric / Description", "Amount"]],
    body: financialRows,
    styles: A4_TABLE_STYLES.styles,
    headStyles: A4_TABLE_STYLES.headStyles,
    columnStyles: { 1: { halign: "right", fontStyle: "bold" } },
    theme: "striped",
    foot: [["Net Profit", money(overview.netProfitMinor)]],
    footStyles: A4_TABLE_STYLES.footStyles,
  });
  y = lastTableEndY(doc, y) + 10;

  // --- Payment Methods Breakdown ---------------------------------------
  if (report.paymentMethods) {
    y = ensureSectionRoom(doc, y);
    drawA4SectionTitle(doc, "Payment Methods Summary", y);
    y += 4;

    const pm = report.paymentMethods;
    const totalSales = overview.totalSalesMinor || 1;

    const calcPct = (amount: number) => {
      if (overview.totalSalesMinor <= 0) return "0.0%";
      return `${((amount / totalSales) * 100).toFixed(1)}%`;
    };

    const paymentRows: Array<[string, string, string, string]> = [
      ["Cash", String(pm.cashCount), money(pm.cashMinor), calcPct(pm.cashMinor)],
    ];
    if (pm.cardCount > 0 || pm.cardMinor > 0) {
      paymentRows.push(["Card", String(pm.cardCount), money(pm.cardMinor), calcPct(pm.cardMinor)]);
    }
    if (pm.otherCount > 0 || pm.otherMinor > 0) {
      paymentRows.push(["Other / Digital", String(pm.otherCount), money(pm.otherMinor), calcPct(pm.otherMinor)]);
    }

    autoTable(doc, {
      startY: y,
      margin: { left: margin, right: margin },
      head: [["Payment Method", "Orders / Txns", "Total Amount", "Share of Sales"]],
      body: paymentRows,
      styles: A4_TABLE_STYLES.styles,
      headStyles: A4_TABLE_STYLES.headStyles,
      columnStyles: {
        1: { halign: "center" },
        2: { halign: "right", fontStyle: "bold" },
        3: { halign: "right" },
      },
      theme: "grid",
      foot: [["Total", String(overview.transactionCount), money(overview.totalSalesMinor), "100.0%"]],
      footStyles: A4_TABLE_STYLES.footStyles,
    });
    y = lastTableEndY(doc, y) + 10;
  }

  // --- Refunds Summary -------------------------------------------------
  y = ensureSectionRoom(doc, y);
  drawA4SectionTitle(doc, "Refunds Summary", y);
  y += 4;

  if (report.refunds.refunds.length === 0) {
    doc.setFont("helvetica", "normal");
    doc.setFontSize(9);
    doc.setTextColor(120);
    doc.text("No refunds recorded for this period.", margin, y + 2);
    doc.setTextColor(0);
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
  }

  return doc;
}

export function downloadFullReportPdf(report: FullReport, config: AppConfig): Promise<boolean> {
  const filename =
    report.startDate === report.endDate
      ? `daily-sales-report-${report.startDate}.pdf`
      : `sales-report-${report.startDate}-to-${report.endDate}.pdf`;
  return downloadPdf(buildFullReportPdf(report, config), filename);
}
