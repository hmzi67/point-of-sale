import type { jsPDF } from "jspdf";
import { formatMinor } from "./format";
import { downloadPdf } from "./pdfExport";
import {
  drawFooter,
  drawHeader,
  drawKeyValueRows,
  drawLogo,
  drawTable,
  drawTotalsBlock,
  loadImageDimensions,
  LOGO_MAX_HEIGHT_MM,
  MARGIN_MM,
  newReceiptDoc,
  PAGE_WIDTH_MM,
} from "./printLayout";
import type { AppConfig, Sale } from "../types";

/** Resolves `config.logoPath` to a `{ dataUrl, width, height }` triple ready
 * for `drawLogo`, or `null` if there's no logo set, the file is missing, or
 * it fails to load for any reason — the same "skip cleanly, never break the
 * receipt" fallback the ESC/POS path takes (see
 * `printer::escpos::build_logo_raster`'s doc comment) rather than showing a
 * broken-image placeholder on a printed receipt. */
async function resolveLogo(logoPath: string | null): Promise<{ dataUrl: string; width: number; height: number } | null> {
  if (!logoPath) return null;
  try {
    const { getLogoDataUrl } = await import("../services/configService");
    const dataUrl = await getLogoDataUrl(logoPath);
    const { width, height } = await loadImageDimensions(dataUrl);
    return { dataUrl, width, height };
  } catch {
    return null;
  }
}

/** A bold, boxed "MERCHANT COPY" banner right under the logo — the PDF
 * equivalent of `printer::escpos::copy_label_banner`'s ASCII double-rule
 * banner. A PDF has real typography/box-drawing available, so this uses
 * an actual bordered rectangle rather than trying to imitate `====` text
 * rules — "use your judgment on what's cleanest for a PDF context"
 * covers exactly this. Returns the y position to continue drawing from. */
function drawCopyLabelBanner(doc: jsPDF, y: number, label: string): number {
  const boxHeight = 8;
  doc.setDrawColor(15, 23, 42);
  doc.setLineWidth(0.5);
  doc.rect(MARGIN_MM, y, PAGE_WIDTH_MM - MARGIN_MM * 2, boxHeight);
  doc.setFont("helvetica", "bold");
  doc.setFontSize(11);
  doc.text(label, PAGE_WIDTH_MM / 2, y + boxHeight / 2 + 1.5, { align: "center" });
  doc.setFont("helvetica", "normal");
  return y + boxHeight + 4;
}

/**
 * Draws one receipt's full content (logo, optional copy-label banner,
 * header, items, totals, footer) onto whatever page `doc` is currently on
 * — the one place that content is actually laid out, so
 * `buildReceiptPdf`/`buildReceiptPdfWithMerchantCopy` below can never
 * drift apart in what they show, mirroring
 * `printer::escpos::build_receipt_bytes_for_copy` on the ESC/POS side.
 */
async function drawReceiptContent(
  doc: jsPDF,
  sale: Sale,
  config: AppConfig,
  tablesEnabled: boolean,
  logo: { dataUrl: string; width: number; height: number } | null,
  copyLabel?: string,
): Promise<void> {
  let y = logo ? drawLogo(doc, logo.dataUrl, logo.width, logo.height) : 6;
  if (copyLabel) y = drawCopyLabelBanner(doc, y, copyLabel);
  y = drawHeader(doc, config.businessName, [`Sale #${sale.id}`, sale.createdAt], y);

  // Cashier / table-or-order-type — a clean label/value row block, same
  // alignment convention `drawTotalsBlock` uses further down, rather than
  // folded into the centered masthead prose above.
  const orderLabel = sale.tableName ? "Table" : "Order Type";
  const orderValue = sale.tableName ?? (tablesEnabled ? "Takeaway" : "Counter Sale");
  y = drawKeyValueRows(doc, y, [
    ["Cashier", sale.cashierName ?? "—"],
    [orderLabel, orderValue],
  ]);
  y += 4;

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
}

function estimatedHeightMm(sale: Sale, config: AppConfig, hasLogo: boolean, hasCopyLabel: boolean): number {
  // logo + optional banner + header block + info rows + one row per item +
  // totals block + footer.
  return (
    44 +
    (hasLogo ? LOGO_MAX_HEIGHT_MM + 3 : 0) +
    (hasCopyLabel ? 12 : 0) +
    10 +
    sale.items.length * 6 +
    36 +
    (config.receiptFooter.trim() ? 10 : 0)
  );
}

/**
 * Builds a compact 80mm-wide receipt PDF — the customer copy only, one
 * page. Returns the `jsPDF` document; call `.save(name)` to trigger a
 * download or `.output(...)` for other uses (e.g. a preview).
 *
 * `tablesEnabled` decides the label shown when `sale.tableName` is unset:
 * "Takeaway" if the shop uses tables at all (this sale just wasn't linked to
 * one), "Counter Sale" if the `tables` module isn't in use for this
 * installation — mirrors the same fallback on the ESC/POS side.
 */
export async function buildReceiptPdf(sale: Sale, config: AppConfig, tablesEnabled: boolean): Promise<jsPDF> {
  const logo = await resolveLogo(config.logoPath);
  const doc = newReceiptDoc(estimatedHeightMm(sale, config, logo !== null, false));
  await drawReceiptContent(doc, sale, config, tablesEnabled, logo);
  return doc;
}

/**
 * The PDF fallback used when no thermal printer is reachable: the same
 * customer receipt as `buildReceiptPdf`, plus a second page carrying the
 * "MERCHANT COPY" banner — a PDF's equivalent of the thermal path's two
 * back-to-back printed copies (`printer::escpos::print_receipt`). Both
 * pages are drawn by the exact same `drawReceiptContent`, so there is no
 * second, divergent receipt template here either.
 */
export async function buildReceiptPdfWithMerchantCopy(sale: Sale, config: AppConfig, tablesEnabled: boolean): Promise<jsPDF> {
  const logo = await resolveLogo(config.logoPath);
  const pageHeight = estimatedHeightMm(sale, config, logo !== null, true);

  const doc = newReceiptDoc(pageHeight);
  await drawReceiptContent(doc, sale, config, tablesEnabled, logo);

  doc.addPage([PAGE_WIDTH_MM, pageHeight]);
  await drawReceiptContent(doc, sale, config, tablesEnabled, logo, "MERCHANT COPY");

  return doc;
}

/** Builds and saves the two-page (customer + merchant copy) receipt PDF —
 * the automatic fallback when thermal printing fails or no printer is
 * configured; see `BillingPage.tsx`'s `completeSale`. */
export async function downloadReceiptPdf(sale: Sale, config: AppConfig, tablesEnabled: boolean): Promise<boolean> {
  const doc = await buildReceiptPdfWithMerchantCopy(sale, config, tablesEnabled);
  return downloadPdf(doc, `receipt-${sale.id}.pdf`);
}
