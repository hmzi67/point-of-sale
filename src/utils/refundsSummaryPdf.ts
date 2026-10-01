import type { jsPDF } from "jspdf";
import { formatMinor } from "./format";
import { downloadPdf } from "./pdfExport";
import {
  drawBoldRow,
  drawFooter,
  drawGrandTotalRow,
  drawHeader,
  drawSectionHeader,
  drawTable,
  estimateReceiptHeightMm,
  MARGIN_MM,
  newReceiptDoc,
  PAGE_WIDTH_MM,
} from "./printLayout";
import type { AppConfig, RefundsSummary } from "../types";

/** Builds the "Refunds Report" PDF: one section per refund (Vno, timestamp,
 * processed by, an Item/Qty/Amount table, the reason if one was given, and
 * that refund's own subtotal), then a grand total at the end — the
 * PDF-fallback twin of `printer::escpos::build_refunds_summary_bytes`, same
 * per-section-then-grand-total shape `categorySalesPdf.ts` uses. */
export function buildRefundsSummaryPdf(report: RefundsSummary, config: AppConfig): jsPDF {
  const itemRowCount = report.refunds.reduce((sum, r) => sum + r.items.length, 0);
  const estimatedHeightMm =
    estimateReceiptHeightMm(report.refunds.length * 18 + itemRowCount * 6 + 20, config.receiptFooter);
  const doc = newReceiptDoc(estimatedHeightMm);

  let y = drawHeader(doc, config.businessName, ["Refunds Report", `${report.startDate} to ${report.endDate}`]);

  if (report.refunds.length === 0) {
    doc.setFontSize(9);
    doc.setTextColor(120);
    doc.text("No refunds in this period", PAGE_WIDTH_MM / 2, y, { align: "center" });
    doc.setTextColor(0);
    y += 8;
  }

  for (const refund of report.refunds) {
    y = drawSectionHeader(doc, y, `Refund #${refund.id}  Vno: ${refund.originalSaleId}`);

    doc.setFontSize(8);
    doc.setTextColor(100);
    const infoLine = [refund.createdAt, refund.refundedByName ? `By: ${refund.refundedByName}` : null]
      .filter(Boolean)
      .join("   ·   ");
    doc.text(infoLine, MARGIN_MM, y);
    doc.setTextColor(0);
    y += 4;

    y = drawTable(
      doc,
      y,
      ["Item", "Qty", "Amount"],
      refund.items.map((line) => [line.itemName, String(line.qtyRefunded), formatMinor(line.amountRefundedMinor, config.currency)]),
    );

    if (refund.reason) {
      doc.setFontSize(8);
      doc.text(`Reason: ${refund.reason}`, MARGIN_MM, y, { maxWidth: PAGE_WIDTH_MM - MARGIN_MM * 2 });
      y += 5;
    }

    y = drawBoldRow(doc, y, "Refund Total", formatMinor(refund.totalRefundAmountMinor, config.currency));
  }

  y = drawGrandTotalRow(doc, y, "TOTAL REFUNDED", formatMinor(report.grandTotalRefundedMinor, config.currency), "heavy");

  drawFooter(doc, y, config.receiptFooter);
  return doc;
}

export function downloadRefundsSummaryPdf(report: RefundsSummary, config: AppConfig): Promise<boolean> {
  return downloadPdf(buildRefundsSummaryPdf(report, config), `refunds-report-${report.startDate}-to-${report.endDate}.pdf`);
}
