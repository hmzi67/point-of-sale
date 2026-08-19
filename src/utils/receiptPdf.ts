import { jsPDF } from "jspdf";
import autoTable from "jspdf-autotable";
import { formatMinor } from "./format";
import type { AppConfig, Sale } from "../types";

const PAGE_WIDTH_MM = 80; // standard thermal-roll width
const MARGIN_MM = 4;

/**
 * Builds a compact 80mm-wide receipt PDF — the default/fallback receipt path,
 * since it works identically with or without a thermal printer attached.
 * Returns the `jsPDF` document; call `.save(name)` to trigger a download or
 * `.output(...)` for other uses (e.g. a preview).
 */
export function buildReceiptPdf(sale: Sale, config: AppConfig): jsPDF {
  // Rough content height so the page isn't needlessly long or clipped:
  // header block + one row per item + totals block + footer.
  const estimatedHeightMm = 42 + sale.items.length * 6 + 34 + (config.receiptFooter.trim() ? 10 : 0);
  const doc = new jsPDF({ unit: "mm", format: [PAGE_WIDTH_MM, Math.max(estimatedHeightMm, 90)] });

  const centerX = PAGE_WIDTH_MM / 2;
  let y = 8;

  doc.setFont("helvetica", "bold");
  doc.setFontSize(12);
  doc.text(config.businessName, centerX, y, { align: "center" });
  y += 6;

  doc.setFont("helvetica", "normal");
  doc.setFontSize(8);
  doc.text(`Sale #${sale.id}`, centerX, y, { align: "center" });
  y += 4;
  doc.text(sale.createdAt, centerX, y, { align: "center" });
  y += 4;
  if (sale.tableName) {
    doc.text(`Table: ${sale.tableName}`, centerX, y, { align: "center" });
    y += 4;
  }
  if (sale.cashierName) {
    doc.text(`Cashier: ${sale.cashierName}`, centerX, y, { align: "center" });
    y += 4;
  }
  y += 2;

  autoTable(doc, {
    startY: y,
    margin: { left: MARGIN_MM, right: MARGIN_MM },
    head: [["Item", "Qty", "Total"]],
    body: sale.items.map((line) => [
      line.itemName,
      String(line.qty),
      formatMinor(line.lineTotalMinor, config.currency),
    ]),
    styles: { fontSize: 8, cellPadding: 1.2 },
    headStyles: { fillColor: [30, 41, 59] },
    theme: "grid",
  });

  // jspdf-autotable augments the document with this at runtime; there is no
  // typed accessor for it, so read it through a narrow, explicit cast rather
  // than an untyped `any` spread over the rest of the function.
  const finalY = (doc as unknown as { lastAutoTable?: { finalY: number } }).lastAutoTable?.finalY;
  y = (finalY ?? y) + 6;

  doc.setFontSize(9);
  const totalsRows: Array<[string, string]> = [
    ["Subtotal", formatMinor(sale.subtotalMinor, config.currency)],
  ];
  if (sale.discountMinor > 0) {
    totalsRows.push(["Discount", `-${formatMinor(sale.discountMinor, config.currency)}`]);
  }
  if (sale.taxMinor > 0) {
    totalsRows.push(["Tax", formatMinor(sale.taxMinor, config.currency)]);
  }
  for (const [label, value] of totalsRows) {
    doc.text(label, MARGIN_MM, y);
    doc.text(value, PAGE_WIDTH_MM - MARGIN_MM, y, { align: "right" });
    y += 5;
  }

  doc.setFont("helvetica", "bold");
  doc.setFontSize(11);
  doc.text("Total", MARGIN_MM, y);
  doc.text(formatMinor(sale.totalMinor, config.currency), PAGE_WIDTH_MM - MARGIN_MM, y, { align: "right" });
  y += 6;

  doc.setFont("helvetica", "normal");
  doc.setFontSize(8);
  doc.text(`Paid by ${sale.paymentMethod}`, centerX, y, { align: "center" });
  y += 6;

  if (config.receiptFooter.trim()) {
    doc.text(config.receiptFooter, centerX, y, {
      align: "center",
      maxWidth: PAGE_WIDTH_MM - MARGIN_MM * 2,
    });
  }

  return doc;
}

/** Builds and immediately downloads the receipt PDF. */
export function downloadReceiptPdf(sale: Sale, config: AppConfig): void {
  buildReceiptPdf(sale, config).save(`receipt-${sale.id}.pdf`);
}
