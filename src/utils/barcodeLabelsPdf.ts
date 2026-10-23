import { jsPDF } from "jspdf";
import { code128Modules } from "./code128";
import { formatMinor } from "./format";
import type { LabelOptions } from "./labelOptions";
import { downloadPdf } from "./pdfExport";

export interface LabelItem {
  name: string;
  barcode: string;
  priceMinor: number;
}

const SHEET = { width: 210, height: 297, margin: 8, gap: 2 };
const PAD = 1.5;

function drawLabel(doc: jsPDF, item: LabelItem, x: number, y: number, options: LabelOptions, currency: string) {
  const { widthMm: w, heightMm: h } = options;
  let top = y + PAD;
  let bottom = y + h - PAD;

  if (options.showName) {
    doc.setFont("helvetica", "bold");
    doc.setFontSize(7);
    const name = doc.splitTextToSize(item.name, w - PAD * 2)[0] as string;
    doc.text(name, x + w / 2, top + 2.4, { align: "center" });
    top += 3.4;
  }
  if (options.showPrice) {
    doc.setFont("helvetica", "bold");
    doc.setFontSize(9);
    doc.text(formatMinor(item.priceMinor, currency), x + w / 2, bottom, { align: "center" });
    bottom -= 3.6;
  }
  if (options.showCode) {
    doc.setFont("courier", "normal");
    doc.setFontSize(6.5);
    doc.text(item.barcode, x + w / 2, bottom, { align: "center" });
    bottom -= 2.4;
  }

  // Ten modules of quiet zone each side, so a scanner finds the edges.
  const modules = code128Modules(item.barcode);
  const total = modules.reduce((a, b) => a + b, 0) + 20;
  const moduleWidth = (w - PAD * 2) / total;
  const barHeight = Math.max(4, bottom - top);
  let cursor = x + PAD + moduleWidth * 10;
  doc.setFillColor(0, 0, 0);
  modules.forEach((width, i) => {
    if (i % 2 === 0) doc.rect(cursor, top, width * moduleWidth, barHeight, "F");
    cursor += width * moduleWidth;
  });
}

/** One PDF with `copies` labels per item. */
export function buildLabelsPdf(items: LabelItem[], options: LabelOptions, currency: string): jsPDF {
  const labels = items.flatMap((item) => Array.from({ length: Math.max(1, options.copies) }, () => item));
  const { widthMm: w, heightMm: h } = options;

  if (options.layout === "roll") {
    const doc = new jsPDF({ unit: "mm", format: [w, h], orientation: w > h ? "landscape" : "portrait" });
    labels.forEach((item, i) => {
      if (i > 0) doc.addPage([w, h], w > h ? "landscape" : "portrait");
      drawLabel(doc, item, 0, 0, options, currency);
    });
    return doc;
  }

  const doc = new jsPDF({ unit: "mm", format: "a4" });
  const columns = Math.max(1, Math.floor((SHEET.width - SHEET.margin * 2 + SHEET.gap) / (w + SHEET.gap)));
  const rows = Math.max(1, Math.floor((SHEET.height - SHEET.margin * 2 + SHEET.gap) / (h + SHEET.gap)));
  labels.forEach((item, i) => {
    const slot = i % (columns * rows);
    if (i > 0 && slot === 0) doc.addPage();
    const col = slot % columns;
    const row = Math.floor(slot / columns);
    const x = SHEET.margin + col * (w + SHEET.gap);
    const y = SHEET.margin + row * (h + SHEET.gap);
    doc.setDrawColor(220, 220, 220);
    doc.rect(x, y, w, h, "S");
    drawLabel(doc, item, x, y, options, currency);
  });
  return doc;
}

export function downloadLabelsPdf(items: LabelItem[], options: LabelOptions, currency: string): Promise<boolean> {
  return downloadPdf(buildLabelsPdf(items, options, currency), "barcode-labels.pdf");
}
