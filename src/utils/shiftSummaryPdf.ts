import type { jsPDF } from "jspdf";
import { formatMinor } from "./format";
import { drawFooter, drawHeader, drawTotalsBlock, newReceiptDoc, PAGE_WIDTH_MM } from "./printLayout";
import type { AppConfig, ShiftSummary } from "../types";

/** Builds the "Counter-N Sale Details" shift close-out receipt PDF — the
 * PDF-fallback twin of `printer::escpos::build_shift_summary_bytes`. */
export function buildShiftSummaryPdf(summary: ShiftSummary, config: AppConfig): jsPDF {
  const doc = newReceiptDoc(110);
  const money = (minor: number) => formatMinor(minor, config.currency);

  const subtitle = [`Counter-${summary.shift.id} Sale Details`, `Opened: ${summary.shift.openedAt}`];
  if (summary.shift.closedAt) subtitle.push(`Closed: ${summary.shift.closedAt}`);
  if (summary.shift.cashierName) subtitle.push(`Cashier: ${summary.shift.cashierName}`);
  let y = drawHeader(doc, config.businessName, subtitle);

  const rows: Array<[string, string]> = [
    ["Opening Balance", money(summary.openingBalanceMinor)],
    ["Cash Sale", money(summary.cashSalesMinor)],
    ["Card Sale", money(summary.cardSalesMinor)],
    ["Credit Sale", money(summary.creditSalesMinor)],
    ["Other Sale", money(summary.otherSalesMinor)],
    ["Discount Today", money(summary.discountMinor)],
    ["Refund Today", money(summary.refundsMinor)],
  ];
  y = drawTotalsBlock(doc, y, rows, "Total Sale", money(summary.totalSalesMinor));

  doc.setFontSize(9);
  doc.text("Expected Cash", 4, y);
  doc.text(money(summary.expectedCashMinor), PAGE_WIDTH_MM - 4, y, { align: "right" });
  y += 5;

  if (summary.declaredCashAmountMinor !== null) {
    doc.text("Declared Amount", 4, y);
    doc.text(money(summary.declaredCashAmountMinor), PAGE_WIDTH_MM - 4, y, { align: "right" });
    y += 5;
  }

  if (summary.differenceMinor !== null) {
    y += 2;
    doc.setDrawColor(203, 213, 225);
    doc.line(4, y, PAGE_WIDTH_MM - 4, y);
    y += 5;
    doc.setFont("helvetica", "bold");
    doc.setFontSize(11);
    const label = summary.differenceMinor < 0 ? "Short" : "Over";
    doc.text(label, 4, y);
    doc.text(money(Math.abs(summary.differenceMinor)), PAGE_WIDTH_MM - 4, y, { align: "right" });
    doc.setFont("helvetica", "normal");
    y += 6;
  }

  drawFooter(doc, y, config.receiptFooter);
  return doc;
}

export function downloadShiftSummaryPdf(summary: ShiftSummary, config: AppConfig): void {
  buildShiftSummaryPdf(summary, config).save(`shift-${summary.shift.id}.pdf`);
}
