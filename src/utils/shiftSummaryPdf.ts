import type { jsPDF } from "jspdf";
import { formatMinor } from "./format";
import { downloadPdf } from "./pdfExport";
import { drawFooter, drawGrandTotalRow, drawHeader, drawKeyValueRows, drawTotalsBlock, newReceiptDoc } from "./printLayout";
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

  const cashRows: Array<[string, string]> = [["Expected Cash", money(summary.expectedCashMinor)]];
  if (summary.declaredCashAmountMinor !== null) {
    cashRows.push(["Declared Amount", money(summary.declaredCashAmountMinor)]);
  }
  y = drawKeyValueRows(doc, y, cashRows);

  if (summary.differenceMinor !== null) {
    const label = summary.differenceMinor < 0 ? "Short" : "Over";
    y = drawGrandTotalRow(doc, y + 2, label, money(Math.abs(summary.differenceMinor)));
  }

  drawFooter(doc, y, config.receiptFooter);
  return doc;
}

export function downloadShiftSummaryPdf(summary: ShiftSummary, config: AppConfig): Promise<boolean> {
  return downloadPdf(buildShiftSummaryPdf(summary, config), `shift-${summary.shift.id}.pdf`);
}
