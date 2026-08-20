import { jsPDF } from "jspdf";
import autoTable from "jspdf-autotable";

/**
 * Shared jsPDF layout primitives — header block, totals block, footer, the
 * standard 80mm-thermal-shaped page — used by every print template
 * (`receiptPdf.ts`, `refundPdf.ts`, `shiftSummaryPdf.ts`,
 * `categorySalesPdf.ts`) so each one only describes *what* goes on the
 * page, not re-derives page width, margins or how a totals row lines up.
 *
 * `PAGE_WIDTH_MM = 80` targets the same physical paper width
 * `printer::layout::LINE_WIDTH` (48 characters) targets on the ESC/POS
 * side — a PDF lays out proportionally in millimeters rather than a fixed
 * character grid, so the two aren't the same unit, but they're sized for
 * the same roll of paper.
 */
export const PAGE_WIDTH_MM = 80;
export const MARGIN_MM = 4;

export function newReceiptDoc(estimatedHeightMm: number): jsPDF {
  return new jsPDF({ unit: "mm", format: [PAGE_WIDTH_MM, Math.max(estimatedHeightMm, 90)] });
}

/** Business name (bold, centered) plus `subtitleLines` centered underneath
 * it. Returns the y position to continue drawing from. */
export function drawHeader(doc: jsPDF, businessName: string, subtitleLines: string[], startY = 8): number {
  const centerX = PAGE_WIDTH_MM / 2;
  let y = startY;

  doc.setFont("helvetica", "bold");
  doc.setFontSize(12);
  doc.text(businessName, centerX, y, { align: "center" });
  y += 6;

  doc.setFont("helvetica", "normal");
  doc.setFontSize(8);
  for (const line of subtitleLines) {
    doc.text(line, centerX, y, { align: "center" });
    y += 4;
  }
  return y + 2;
}

/** A bold, centered section title (e.g. a category header, "REFUND"). */
export function drawSectionHeader(doc: jsPDF, y: number, title: string): number {
  doc.setFont("helvetica", "bold");
  doc.setFontSize(10);
  doc.text(title, MARGIN_MM, y);
  doc.setFont("helvetica", "normal");
  return y + 4;
}

/** The tabular item/line list every template uses — same styling as the
 * original receipt table (dark header fill, grid theme), just parameterized
 * over columns/rows so a template can add a Rate column or drop Qty. */
export function drawTable(doc: jsPDF, startY: number, head: string[], body: string[][]): number {
  autoTable(doc, {
    startY,
    margin: { left: MARGIN_MM, right: MARGIN_MM },
    head: [head],
    body,
    styles: { fontSize: 8, cellPadding: 1.2 },
    headStyles: { fillColor: [30, 41, 59] },
    theme: "grid",
    // Right-align every numeric-looking column (all but the first) — the
    // "Description / Qty / Rate / Amount, right-aligned numbers" convention
    // every template follows.
    columnStyles: Object.fromEntries(head.slice(1).map((_, i) => [i + 1, { halign: "right" as const }])),
  });

  // jspdf-autotable augments the document with this at runtime; there is no
  // typed accessor for it, so read it through a narrow, explicit cast rather
  // than an untyped `any` spread over the rest of the function.
  const finalY = (doc as unknown as { lastAutoTable?: { finalY: number } }).lastAutoTable?.finalY;
  return (finalY ?? startY) + 6;
}

/** A block of label/value rows, then a bold, larger grand-total row under a
 * rule — the totals section every template ends with. */
export function drawTotalsBlock(
  doc: jsPDF,
  startY: number,
  rows: Array<[string, string]>,
  grandLabel: string,
  grandValue: string,
): number {
  let y = startY;
  doc.setFontSize(9);
  for (const [label, value] of rows) {
    doc.text(label, MARGIN_MM, y);
    doc.text(value, PAGE_WIDTH_MM - MARGIN_MM, y, { align: "right" });
    y += 5;
  }

  doc.setDrawColor(203, 213, 225); // slate-300
  doc.line(MARGIN_MM, y, PAGE_WIDTH_MM - MARGIN_MM, y);
  y += 5;

  doc.setFont("helvetica", "bold");
  doc.setFontSize(11);
  doc.text(grandLabel, MARGIN_MM, y);
  doc.text(grandValue, PAGE_WIDTH_MM - MARGIN_MM, y, { align: "right" });
  doc.setFont("helvetica", "normal");
  return y + 6;
}

/** The centered footer text every template ends with, if configured. */
export function drawFooter(doc: jsPDF, y: number, footer: string): void {
  if (!footer.trim()) return;
  doc.setFontSize(8);
  doc.text(footer, PAGE_WIDTH_MM / 2, y, {
    align: "center",
    maxWidth: PAGE_WIDTH_MM - MARGIN_MM * 2,
  });
}
