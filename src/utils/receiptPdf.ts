import type { jsPDF } from "jspdf";
import { formatMinor } from "./format";
import {
  drawFooter,
  drawHeader,
  drawKeyValueRows,
  drawLogo,
  drawTable,
  drawTotalsBlock,
  loadImageDimensions,
  LOGO_MAX_HEIGHT_MM,
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

/**
 * Builds a compact 80mm-wide receipt PDF — the default/fallback receipt path,
 * since it works identically with or without a thermal printer attached.
 * Returns the `jsPDF` document; call `.save(name)` to trigger a download or
 * `.output(...)` for other uses (e.g. a preview).
 *
 * `tablesEnabled` decides the label shown when `sale.tableName` is unset:
 * "Takeaway" if the shop uses tables at all (this sale just wasn't linked to
 * one), "Counter Sale" if the `tables` module isn't in use for this
 * installation — mirrors the same fallback on the ESC/POS side.
 */
export async function buildReceiptPdf(sale: Sale, config: AppConfig, tablesEnabled: boolean): Promise<jsPDF> {
  const logo = await resolveLogo(config.logoPath);

  // Rough content height so the page isn't needlessly long or clipped:
  // logo + header block + info rows + one row per item + totals block + footer.
  const estimatedHeightMm =
    44 + (logo ? LOGO_MAX_HEIGHT_MM + 3 : 0) + 10 + sale.items.length * 6 + 36 + (config.receiptFooter.trim() ? 10 : 0);
  const doc = newReceiptDoc(estimatedHeightMm);

  let y = logo ? drawLogo(doc, logo.dataUrl, logo.width, logo.height) : 6;
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
  return doc;
}

/** Builds and immediately downloads the receipt PDF. */
export async function downloadReceiptPdf(sale: Sale, config: AppConfig, tablesEnabled: boolean): Promise<void> {
  const doc = await buildReceiptPdf(sale, config, tablesEnabled);
  doc.save(`receipt-${sale.id}.pdf`);
}
