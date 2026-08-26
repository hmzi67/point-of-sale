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

/** Max box a receipt logo is drawn into, aspect ratio preserved. Bumped up
 * from a previous 26×16mm now that the receipt no longer prints the
 * business name as text (see `receiptPdf.ts`'s `drawReceiptContent`) — the
 * logo is the receipt's only brand mark now, so it reads as a real header
 * rather than a small corner mark. Still well inside `PAGE_WIDTH_MM`'s
 * 80mm, with margin either side. */
export const LOGO_MAX_WIDTH_MM = 40;
export const LOGO_MAX_HEIGHT_MM = 24;

/** Resolves a `data:` URL's natural pixel dimensions — needed to draw it into
 * the PDF at the right aspect ratio instead of stretching/squashing it to a
 * fixed box. Rejects on a load failure (a corrupt or unreadable data URL)
 * rather than hanging forever. */
export function loadImageDimensions(dataUrl: string): Promise<{ width: number; height: number }> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve({ width: img.naturalWidth, height: img.naturalHeight });
    img.onerror = () => reject(new Error("Could not read the logo image"));
    img.src = dataUrl;
  });
}

/**
 * Draws `dataUrl` centered at the top of the page, scaled to fit within
 * `LOGO_MAX_WIDTH_MM` × `LOGO_MAX_HEIGHT_MM` while preserving its aspect
 * ratio. Returns the y position to continue drawing from (`startY`
 * unchanged if nothing was drawn).
 *
 * Skips entirely for an SVG data URL — jsPDF's `addImage` only rasterizes
 * PNG/JPEG/WEBP, not SVG, so an SVG logo (allowed for the on-screen logo,
 * see `images::LOGO_ALLOWED_EXTENSIONS` on the Rust side) just doesn't
 * appear on the PDF receipt. Same graceful "no logo" fallback the ESC/POS
 * path takes for the same underlying reason — see
 * `printer::escpos::build_logo_raster`'s doc comment.
 */
export function drawLogo(
  doc: jsPDF,
  dataUrl: string,
  naturalWidth: number,
  naturalHeight: number,
  startY = 6,
): number {
  if (dataUrl.startsWith("data:image/svg") || naturalWidth <= 0 || naturalHeight <= 0) return startY;

  const aspect = naturalWidth / naturalHeight;
  let width = LOGO_MAX_WIDTH_MM;
  let height = width / aspect;
  if (height > LOGO_MAX_HEIGHT_MM) {
    height = LOGO_MAX_HEIGHT_MM;
    width = height * aspect;
  }

  doc.addImage(dataUrl, (PAGE_WIDTH_MM - width) / 2, startY, width, height);
  return startY + height + 3;
}

/** Business name (bold, centered), if given, plus `subtitleLines` centered
 * underneath it. Returns the y position to continue drawing from.
 * `businessName` is `string | null` — the sale receipt (`receiptPdf.ts`)
 * passes `null` deliberately (the logo is its brand mark now, not text);
 * every other template (refund, reports) still passes the real name. */
export function drawHeader(doc: jsPDF, businessName: string | null, subtitleLines: string[], startY = 8): number {
  const centerX = PAGE_WIDTH_MM / 2;
  let y = startY;

  if (businessName) {
    doc.setFont("helvetica", "bold");
    doc.setFontSize(12);
    doc.text(businessName, centerX, y, { align: "center" });
    y += 6;
  }

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

/** The tabular item/line list every template uses — a real, crisp grid
 * (every cell bordered, header and body alike, Excel-style — not just an
 * outer box) — parameterized over columns/rows so a template can add a
 * Rate column or drop Qty. */
export function drawTable(doc: jsPDF, startY: number, head: string[], body: string[][]): number {
  autoTable(doc, {
    startY,
    margin: { left: MARGIN_MM, right: MARGIN_MM },
    head: [head],
    body,
    styles: {
      fontSize: 8,
      cellPadding: 1.6,
      lineWidth: 0.15,
      lineColor: [100, 116, 139], // slate-500 — a real, visible grid line on every cell
    },
    headStyles: { fillColor: [30, 41, 59], textColor: [255, 255, 255], fontStyle: "bold" },
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

/** Plain label-left/value-right rows at a given font size — the "Cashier ...
 * John" / "Subtotal ... 123.45" alignment convention shared by the receipt's
 * info block and `drawTotalsBlock`'s rows below. Returns the y position to
 * continue drawing from. */
export function drawKeyValueRows(doc: jsPDF, startY: number, rows: Array<[string, string]>, fontSize = 9): number {
  let y = startY;
  doc.setFontSize(fontSize);
  for (const [label, value] of rows) {
    doc.text(label, MARGIN_MM, y);
    doc.text(value, PAGE_WIDTH_MM - MARGIN_MM, y, { align: "right" });
    y += 5;
  }
  return y;
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
  let y = drawKeyValueRows(doc, startY, rows);

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
