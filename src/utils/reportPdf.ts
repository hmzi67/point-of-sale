import { jsPDF } from "jspdf";
import autoTable from "jspdf-autotable";
import { formatMinor } from "./format";
import { downloadPdf } from "./pdfExport";
import type { ReportData } from "./reportCsv";

/** Builds an A4 report PDF: header, summary, then the top-items table via
 * jspdf-autotable. Returns the document; call `.save(name)` to download.
 * Kept in its own module (pulled in only via dynamic import) so jsPDF's
 * ~140KB never loads on the reports screen itself — only on export. */
export function buildReportPdf({ summary, topItems, config }: ReportData): jsPDF {
  const doc = new jsPDF({ unit: "mm", format: "a4" });
  const pageWidth = doc.internal.pageSize.getWidth();
  const margin = 14;
  let y = 18;

  doc.setFont("helvetica", "bold");
  doc.setFontSize(16);
  doc.text(`${config.businessName} — Sales Report`, margin, y);
  y += 7;

  doc.setFont("helvetica", "normal");
  doc.setFontSize(10);
  doc.setTextColor(100);
  doc.text(`${summary.startDate} to ${summary.endDate}`, margin, y);
  y += 10;
  doc.setTextColor(0);

  const stats: Array<[string, string]> = [
    ["Total sales", formatMinor(summary.totalSalesMinor, config.currency)],
    ["Transactions", String(summary.transactionCount)],
    ["Average sale", formatMinor(summary.averageSaleMinor, config.currency)],
  ];
  const columnWidth = (pageWidth - margin * 2) / stats.length;
  stats.forEach(([label, value], index) => {
    const x = margin + index * columnWidth;
    doc.setFontSize(9);
    doc.setTextColor(100);
    doc.text(label.toUpperCase(), x, y);
    doc.setFontSize(14);
    doc.setFont("helvetica", "bold");
    doc.setTextColor(0);
    doc.text(value, x, y + 7);
    doc.setFont("helvetica", "normal");
  });
  y += 16;

  doc.setFontSize(12);
  doc.setFont("helvetica", "bold");
  doc.text("Top-selling items", margin, y);
  y += 3;

  autoTable(doc, {
    startY: y,
    margin: { left: margin, right: margin },
    head: [["#", "Item", "Qty sold", "Revenue"]],
    body: topItems.map((item, index) => [
      String(index + 1),
      item.itemName,
      String(item.qtySold),
      formatMinor(item.revenueMinor, config.currency),
    ]),
    styles: { fontSize: 9, cellPadding: 2.5 },
    headStyles: { fillColor: [30, 41, 59] },
    columnStyles: { 0: { cellWidth: 10 }, 2: { halign: "right" }, 3: { halign: "right" } },
    theme: "striped",
  });

  return doc;
}

export function downloadReportPdf(data: ReportData): Promise<boolean> {
  return downloadPdf(buildReportPdf(data), `sales-report-${data.summary.startDate}-to-${data.summary.endDate}.pdf`);
}
