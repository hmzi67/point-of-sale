import type { jsPDF } from "jspdf";
import { formatMinor } from "./format";
import { drawFooter, drawHeader, drawTable, drawTotalsBlock, newReceiptDoc, PAGE_WIDTH_MM } from "./printLayout";
import type { AppConfig, Sale } from "../types";

/**
 * Builds a compact 80mm-wide receipt PDF — the default/fallback receipt path,
 * since it works identically with or without a thermal printer attached.
 * Returns the `jsPDF` document; call `.save(name)` to trigger a download or
 * `.output(...)` for other uses (e.g. a preview).
 */
export function buildReceiptPdf(sale: Sale, config: AppConfig): jsPDF {
  // Rough content height so the page isn't needlessly long or clipped:
  // header block + one row per item + totals block + footer.
  const estimatedHeightMm = 44 + sale.items.length * 6 + 36 + (config.receiptFooter.trim() ? 10 : 0);
  const doc = newReceiptDoc(estimatedHeightMm);

  const subtitle = [`Sale #${sale.id}`, sale.createdAt];
  if (sale.tableName) subtitle.push(`Table: ${sale.tableName}`);
  if (sale.cashierName) subtitle.push(`Cashier: ${sale.cashierName}`);
  let y = drawHeader(doc, config.businessName, subtitle);

  y = drawTable(
    doc,
    y,
    ["Item", "Qty", "Rate", "Amount"],
    sale.items.map((line) => [
      line.itemName,
      String(line.qty),
      formatMinor(line.priceAtSaleMinor, config.currency),
      formatMinor(line.lineTotalMinor, config.currency),
    ]),
  );

  const totalsRows: Array<[string, string]> = [["Subtotal", formatMinor(sale.subtotalMinor, config.currency)]];
  if (sale.discountMinor > 0) {
    totalsRows.push(["Discount", `-${formatMinor(sale.discountMinor, config.currency)}`]);
  }
  if (sale.taxMinor > 0) {
    totalsRows.push(["Tax", formatMinor(sale.taxMinor, config.currency)]);
  }
  y = drawTotalsBlock(doc, y, totalsRows, "TOTAL", formatMinor(sale.totalMinor, config.currency));

  doc.setFontSize(8);
  doc.text(`Paid by ${sale.paymentMethod}`, PAGE_WIDTH_MM / 2, y, { align: "center" });
  y += 6;

  drawFooter(doc, y, config.receiptFooter);
  return doc;
}

/** Builds and immediately downloads the receipt PDF. */
export function downloadReceiptPdf(sale: Sale, config: AppConfig): void {
  buildReceiptPdf(sale, config).save(`receipt-${sale.id}.pdf`);
}
