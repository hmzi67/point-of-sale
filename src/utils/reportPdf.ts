import type { jsPDF } from "jspdf";
import autoTable from "jspdf-autotable";
import { A4_MARGIN_MM, A4_TABLE_STYLES, drawA4SectionTitle, drawA4StatTiles, drawA4Title, newA4Doc } from "./a4Pdf";
import { formatMinor, formatQty } from "./format";
import { downloadPdf } from "./pdfExport";
import type { ReportData } from "./reportCsv";

/** Builds an A4 report PDF: header, summary, then the top-items table via
 * jspdf-autotable. Returns the document; call `.save(name)` to download.
 * Kept in its own module (pulled in only via dynamic import) so jsPDF's
 * ~140KB never loads on the reports screen itself — only on export. */
export function buildReportPdf({ summary, topItems, config }: ReportData): jsPDF {
  const doc = newA4Doc();
  const margin = A4_MARGIN_MM;
  let y = drawA4Title(doc, `${config.businessName} — Sales Report`, `${summary.startDate} to ${summary.endDate}`);

  drawA4StatTiles(
    doc,
    y,
    [
      ["Total sales", formatMinor(summary.totalSalesMinor, config.currency)],
      ["Transactions", String(summary.transactionCount)],
      ["Average sale", formatMinor(summary.averageSaleMinor, config.currency)],
    ],
    14,
  );
  y += 16;

  drawA4SectionTitle(doc, "Top-selling items", y);
  y += 3;

  autoTable(doc, {
    startY: y,
    margin: { left: margin, right: margin },
    head: [["#", "Item", "Qty sold", "Revenue"]],
    body: topItems.map((item, index) => [
      String(index + 1),
      item.itemName,
      formatQty(item.qtySold),
      formatMinor(item.revenueMinor, config.currency),
    ]),
    styles: { fontSize: 9, cellPadding: 2.5 },
    headStyles: A4_TABLE_STYLES.headStyles,
    columnStyles: { 0: { cellWidth: 10 }, 2: { halign: "right" }, 3: { halign: "right" } },
    theme: "striped",
  });

  return doc;
}

export function downloadReportPdf(data: ReportData): Promise<boolean> {
  return downloadPdf(buildReportPdf(data), `sales-report-${data.summary.startDate}-to-${data.summary.endDate}.pdf`);
}
