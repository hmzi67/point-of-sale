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

// --- Thermal-styled receipt primitives -----------------------------------
//
// The customer receipt (`receiptPdf.ts`) is drawn to match the ESC/POS
// thermal output character-for-character in spirit: a monospaced face, a
// boxed contact card, and an item table ruled with horizontal lines only —
// no vertical borders, no filled header band. The helpers below exist for
// that template specifically; every other template (refund, reports) still
// uses `drawTable`/`drawHeader` above and is deliberately untouched.

/** The monospaced face the receipt draws in, mirroring the thermal
 * printer's fixed-pitch Font A. jsPDF ships Courier as a built-in, so this
 * needs no font embedding and adds nothing to the bundle. */
export const MONO_FONT = "courier";

/** Ink color for receipt rules and borders — near-black rather than a light
 * slate, so a rule survives both a screen preview and a cheap inkjet. */
const RULE_COLOR: [number, number, number] = [30, 41, 59];
const RULE_WIDTH = 0.2;

/** A full-width horizontal rule, the PDF counterpart of the thermal
 * `divider()`. Returns the y to continue drawing from. */
export function drawRule(doc: jsPDF, y: number): number {
  doc.setDrawColor(...RULE_COLOR);
  doc.setLineWidth(RULE_WIDTH);
  doc.line(MARGIN_MM, y, PAGE_WIDTH_MM - MARGIN_MM, y);
  return y + 3.4;
}

/** Softer ink, used for footer text that should read as secondary to the
 * transaction above it — still solidly printable, but clearly subordinate
 * to `RULE_COLOR`'s near-black. */
const LIGHT_RULE_COLOR: [number, number, number] = [148, 163, 184]; // slate-400

/** A dashed full-width rule — the PDF counterpart of the thermal
 * `dashed_divider()`. Lighter than `drawRule`'s solid line without dropping
 * to invisibility, for a separation that marks a change of subject rather
 * than a structural boundary.
 *
 * The dash pattern is reset before returning: jsPDF's line style is
 * document-global state, so leaving it set would silently turn every
 * later `drawRule` (and the contact box's own border) dashed too. */
export function drawDashedRule(doc: jsPDF, y: number): number {
  doc.setDrawColor(...RULE_COLOR);
  doc.setLineWidth(RULE_WIDTH);
  doc.setLineDashPattern([0.7, 0.7], 0);
  doc.line(MARGIN_MM, y, PAGE_WIDTH_MM - MARGIN_MM, y);
  doc.setLineDashPattern([], 0);
  return y + 3.4;
}

/** The boxed contact card (Business / Delivery numbers) drawn under the
 * logo — a real rectangle with a horizontal rule between each row, label
 * left and value right, mirroring the thermal `box_row`/`box_line` card.
 * Returns `startY` unchanged if `rows` is empty, so a shop with neither
 * number configured simply gets no box at all. */
export function drawContactBox(doc: jsPDF, startY: number, rows: Array<[string, string]>): number {
  if (rows.length === 0) return startY;

  const rowHeight = 5.4;
  const boxHeight = rows.length * rowHeight;
  const left = MARGIN_MM;
  const right = PAGE_WIDTH_MM - MARGIN_MM;

  doc.setDrawColor(...RULE_COLOR);
  doc.setLineWidth(RULE_WIDTH);
  doc.rect(left, startY, right - left, boxHeight);

  doc.setFont(MONO_FONT, "bold");
  doc.setFontSize(8.5);
  rows.forEach(([label, value], i) => {
    const rowTop = startY + i * rowHeight;
    // A divider between rows, but never above the first (that's the box's
    // own top edge, already drawn by `rect`).
    if (i > 0) doc.line(left, rowTop, right, rowTop);
    const baseline = rowTop + rowHeight / 2 + 1.2;
    doc.text(`${label}:`, left + 2, baseline);
    doc.text(value, right - 2, baseline, { align: "right" });
  });

  return startY + boxHeight + 4;
}

/** Fractions of the content width each item column gets, tracking the
 * thermal `ITEM_TABLE_COLS` ([16, 5, 7, 9] of 37) closely — but with one
 * unit moved from the item name to the rate column.
 *
 * On the thermal grid 7 characters is an *exact* fit for a value like
 * "1300.00", and a character cell already includes its own inter-cell gap,
 * so the number never touches the rule beside it. A PDF has no such cell:
 * a right-aligned 7-glyph number lands flush against the drawn divider and
 * reads as cramped. The extra unit buys that clearance back. */
const ITEM_COL_FRACTIONS = [15 / 37, 5 / 37, 8 / 37, 9 / 37];

/** The receipt's item table: a bold `ITEM QTY RATE AMOUNT` header between
 * two rules, the item rows, then a closing rule. Item name left-aligned,
 * every numeric column right-aligned at its own boundary — the same
 * four-column shape the thermal path prints, with no vertical borders.
 * Long item names are clipped to their column rather than wrapped, matching
 * the thermal `pad`'s truncate-never-wrap rule. */
export function drawRuledItemTable(doc: jsPDF, startY: number, rows: string[][]): number {
  const left = MARGIN_MM;
  const right = PAGE_WIDTH_MM - MARGIN_MM;
  const contentWidth = right - left;
  const widths = ITEM_COL_FRACTIONS.map((f) => f * contentWidth);
  // Boundary x of each column: [left edge, …3 internal dividers…, right edge].
  const bounds = widths.reduce<number[]>((acc, w) => [...acc, acc[acc.length - 1] + w], [left]);

  const CELL_PAD = 1.2;
  const HEADER_HEIGHT = 5.6;
  const ROW_HEIGHT = 5.2;
  const tableHeight = HEADER_HEIGHT + rows.length * ROW_HEIGHT;

  doc.setDrawColor(...RULE_COLOR);
  doc.setLineWidth(RULE_WIDTH);

  // Outer border, then the rule under the header, then one between each
  // pair of item rows — a real grid rather than a list under a line.
  doc.rect(left, startY, contentWidth, tableHeight);
  doc.line(left, startY + HEADER_HEIGHT, right, startY + HEADER_HEIGHT);
  for (let i = 1; i < rows.length; i += 1) {
    const rowTop = startY + HEADER_HEIGHT + i * ROW_HEIGHT;
    doc.line(left, rowTop, right, rowTop);
  }
  // Vertical dividers between columns, full table height.
  for (let i = 1; i < bounds.length - 1; i += 1) {
    doc.line(bounds[i], startY, bounds[i], startY + tableHeight);
  }

  // Item name left-aligned from its cell's left edge; the three numeric
  // columns right-aligned against their cell's right edge, each inset by
  // CELL_PAD so no glyph touches a rule.
  const drawCells = (cells: string[], baseline: number) => {
    doc.text(clipToWidth(doc, cells[0], widths[0] - CELL_PAD * 2), left + CELL_PAD, baseline);
    for (let i = 1; i < cells.length; i += 1) {
      doc.text(cells[i], bounds[i + 1] - CELL_PAD, baseline, { align: "right" });
    }
  };

  doc.setFont(MONO_FONT, "bold");
  doc.setFontSize(8);
  drawCells(["ITEM", "QTY", "RATE", "AMOUNT"], startY + HEADER_HEIGHT - 1.9);

  doc.setFont(MONO_FONT, "normal");
  rows.forEach((row, i) => {
    drawCells(row, startY + HEADER_HEIGHT + i * ROW_HEIGHT + ROW_HEIGHT - 1.7);
  });

  return startY + tableHeight + 4;
}

/** Truncates `text` to whatever fits `maxWidth` millimeters at the current
 * font — the PDF counterpart of the thermal `pad`'s truncation, so a long
 * item name shortens instead of colliding with the Qty column. */
function clipToWidth(doc: jsPDF, text: string, maxWidth: number): string {
  if (doc.getTextWidth(text) <= maxWidth) return text;
  let clipped = text;
  while (clipped.length > 1 && doc.getTextWidth(clipped) > maxWidth) {
    clipped = clipped.slice(0, -1);
  }
  return clipped;
}

/** A label-left/value-right row in the receipt's monospaced face — the
 * totals rows and the sale-info block both use it. */
export function drawMonoRow(
  doc: jsPDF,
  y: number,
  left: string,
  right: string,
  { bold = false, size = 8.5 }: { bold?: boolean; size?: number } = {},
): number {
  doc.setFont(MONO_FONT, bold ? "bold" : "normal");
  doc.setFontSize(size);
  doc.text(left, MARGIN_MM, y);
  doc.text(right, PAGE_WIDTH_MM - MARGIN_MM, y, { align: "right" });
  return y + 4.8;
}

/** Character columns one line of the thermal receipt holds — the Rust
 * side's `printer::layout::LINE_WIDTH`, mirrored here so the PDF can make
 * the *same* fit decisions the printer does (notably whether the developer
 * credit's site and phone share a line). Kept as a named constant rather
 * than an inline number so the two stay visibly coupled: if the thermal
 * budget ever becomes configurable, this is the counterpart to change. */
export const THERMAL_LINE_WIDTH = 42;

/** Centered text in the receipt's monospaced face. `muted` drops the ink to
 * the light rule color, for footer text that should read as secondary to
 * the transaction above it. */
export function drawMonoCentered(
  doc: jsPDF,
  y: number,
  text: string,
  { bold = false, size = 8.5, muted = false }: { bold?: boolean; size?: number; muted?: boolean } = {},
): number {
  doc.setFont(MONO_FONT, bold ? "bold" : "normal");
  doc.setFontSize(size);
  if (muted) doc.setTextColor(...LIGHT_RULE_COLOR);
  doc.text(text, PAGE_WIDTH_MM / 2, y, { align: "center", maxWidth: PAGE_WIDTH_MM - MARGIN_MM * 2 });
  // Text color is document-global state — leaving it set would silently
  // mute everything drawn afterwards.
  if (muted) doc.setTextColor(0, 0, 0);
  return y + size * 0.55 + 1.6;
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
