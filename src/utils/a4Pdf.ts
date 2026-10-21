import { jsPDF } from "jspdf";
import type { UserOptions } from "jspdf-autotable";

/**
 * Shared layout for the full-width A4 report PDFs (`reportPdf.ts`,
 * `fullReportPdf.ts`) — the 80mm thermal-shaped templates use
 * `printLayout.ts` instead. Like those, both builders are only ever pulled
 * in by dynamic import, so this never loads on a screen by itself.
 */
export const A4_MARGIN_MM = 14;
/** Where the first line sits, and where a fresh page starts. */
export const A4_TOP_MM = 18;
/** Below this y a new section starts on a fresh page rather than leaving
 * only its heading at the foot of the current one. */
const A4_SECTION_BREAK_MM = 250;

/** Dark header band, a compact body and the bold light-grey totals row
 * every A4 report table shares. */
/** Slate-800: the dark header band every printed table uses. */
export const TABLE_HEAD_FILL: [number, number, number] = [30, 41, 59];

export const A4_TABLE_STYLES = {
  styles: { fontSize: 8, cellPadding: 1.8 },
  headStyles: { fillColor: TABLE_HEAD_FILL },
  footStyles: { fillColor: [241, 245, 249], textColor: [15, 23, 42], fontStyle: "bold" },
} satisfies Pick<UserOptions, "styles" | "headStyles" | "footStyles">;

export function newA4Doc(): jsPDF {
  return new jsPDF({ unit: "mm", format: "a4" });
}

/** Bold title plus a grey date-range line; returns the y below them. */
export function drawA4Title(doc: jsPDF, title: string, subtitle: string): number {
  let y = A4_TOP_MM;
  doc.setFont("helvetica", "bold");
  doc.setFontSize(16);
  doc.text(title, A4_MARGIN_MM, y);
  y += 7;

  doc.setFont("helvetica", "normal");
  doc.setFontSize(10);
  doc.setTextColor(100);
  doc.text(subtitle, A4_MARGIN_MM, y);
  doc.setTextColor(0);
  return y + 10;
}

/** A row of evenly spaced "LABEL / big value" tiles starting at `y`. */
export function drawA4StatTiles(doc: jsPDF, y: number, stats: Array<[string, string]>, valueFontSize: number): void {
  const pageWidth = doc.internal.pageSize.getWidth();
  const columnWidth = (pageWidth - A4_MARGIN_MM * 2) / stats.length;
  stats.forEach(([label, value], index) => {
    const x = A4_MARGIN_MM + index * columnWidth;
    doc.setFontSize(9);
    doc.setTextColor(100);
    doc.text(label.toUpperCase(), x, y);
    doc.setFontSize(valueFontSize);
    doc.setFont("helvetica", "bold");
    doc.setTextColor(0);
    doc.text(value, x, y + 7);
    doc.setFont("helvetica", "normal");
  });
}

/** Starts a new page when `y` is too close to the bottom for a section to
 * begin; returns the y to continue from. */
export function ensureSectionRoom(doc: jsPDF, y: number): number {
  if (y <= A4_SECTION_BREAK_MM) return y;
  doc.addPage();
  return A4_TOP_MM;
}

/** A bold 12pt section heading. */
export function drawA4SectionTitle(doc: jsPDF, title: string, y: number): void {
  doc.setFont("helvetica", "bold");
  doc.setFontSize(12);
  doc.text(title, A4_MARGIN_MM, y);
}
