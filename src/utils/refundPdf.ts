import type { jsPDF } from "jspdf";
import { formatMinor } from "./format";
import { drawFooter, drawHeader, drawTable, drawTotalsBlock, newReceiptDoc } from "./printLayout";
import type { AppConfig, Refund } from "../types";

/** Builds the "Refund Details" receipt PDF: Vno/item/amount lines, Total
 * Refund at the bottom — the PDF-fallback twin of `printer::escpos::
 * build_refund_bytes`. */
export function buildRefundPdf(refund: Refund, config: AppConfig): jsPDF {
  const estimatedHeightMm = 46 + refund.items.length * 6 + 28 + (config.receiptFooter.trim() ? 10 : 0);
  const doc = newReceiptDoc(estimatedHeightMm);

  const subtitle = ["REFUND", `Refund #${refund.id}`, `Vno: ${refund.originalSaleId}`, refund.createdAt];
  if (refund.refundedByName) subtitle.push(`By: ${refund.refundedByName}`);
  let y = drawHeader(doc, config.businessName, subtitle);

  y = drawTable(
    doc,
    y,
    ["Item", "Qty", "Amount"],
    refund.items.map((line) => [
      line.itemName,
      String(line.qtyRefunded),
      formatMinor(line.amountRefundedMinor, config.currency),
    ]),
  );

  if (refund.reason) {
    doc.setFontSize(8);
    doc.text(`Reason: ${refund.reason}`, 4, y, { maxWidth: 72 });
    y += 6;
  }

  y = drawTotalsBlock(doc, y, [], "Total Refund", formatMinor(refund.totalRefundAmountMinor, config.currency));

  drawFooter(doc, y, config.receiptFooter);
  return doc;
}

export function downloadRefundPdf(refund: Refund, config: AppConfig): void {
  buildRefundPdf(refund, config).save(`refund-${refund.id}.pdf`);
}
