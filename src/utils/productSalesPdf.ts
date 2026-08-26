import type { jsPDF } from "jspdf";
import { formatMinor, formatQty } from "./format";
import { downloadPdf } from "./pdfExport";
import { drawFooter, drawHeader, drawSectionHeader, drawTable, newReceiptDoc } from "./printLayout";
import type { AppConfig, ProductSalesSummaryReport } from "../types";

/** Builds the "Product Wise Sales" report PDF: the ranked "sold" rows (in
 * the report's requested sort order) followed by a "No Sales This Period"
 * section listing active items that sold zero units — the PDF twin of
 * `ProductWiseSalesTable`. */
export function buildProductSalesPdf(report: ProductSalesSummaryReport, config: AppConfig): jsPDF {
  const estimatedHeightMm =
    46 + report.rows.length * 6 + 14 + report.noSalesItems.length * 6 + (config.receiptFooter.trim() ? 10 : 0);
  const doc = newReceiptDoc(estimatedHeightMm);

  const sortLabel = report.sortBy === "revenue" ? "Sorted by revenue" : "Sorted by quantity sold";
  let y = drawHeader(doc, config.businessName, ["Product Wise Sales", `${report.startDate} to ${report.endDate}`, sortLabel]);

  y = drawTable(
    doc,
    y,
    ["Item", "Category", "Qty", "Revenue"],
    // Rank is prefixed onto the item name (column 0, left-aligned) rather
    // than given its own column — `drawTable` right-aligns every column
    // after the first, which would misplace a bare rank number.
    report.rows.map((row) => [
      `${row.rank}. ${row.itemName}`,
      row.categoryName,
      formatQty(row.qtySold),
      formatMinor(row.revenueMinor, config.currency),
    ]),
  );

  if (report.noSalesItems.length > 0) {
    y = drawSectionHeader(doc, y, "No Sales This Period");
    y = drawTable(
      doc,
      y,
      ["Item", "Category"],
      report.noSalesItems.map((row) => [row.itemName, row.categoryName]),
    );
  }

  drawFooter(doc, y, config.receiptFooter);
  return doc;
}

export function downloadProductSalesPdf(report: ProductSalesSummaryReport, config: AppConfig): Promise<boolean> {
  return downloadPdf(buildProductSalesPdf(report, config), `product-wise-sales-${report.startDate}-to-${report.endDate}.pdf`);
}
