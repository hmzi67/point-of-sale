import type { jsPDF } from "jspdf";
import { formatMinor, formatQty } from "./format";
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
 * Draws the receipt's full content (logo, contact info, sale info, items,
 * totals, footer) onto whatever page `doc` is currently on — the one place
 * that content is actually laid out, mirroring `printer::escpos::
 * build_receipt_bytes` on the ESC/POS side.
 *
 * The business name is deliberately not drawn — the logo is the receipt's
 * brand mark now (see `printLayout.ts`'s `LOGO_MAX_WIDTH_MM`).
 */
async function drawReceiptContent(
  doc: jsPDF,
  sale: Sale,
  config: AppConfig,
  tablesEnabled: boolean,
  logo: { dataUrl: string; width: number; height: number } | null,
): Promise<void> {
  let y = logo ? drawLogo(doc, logo.dataUrl, logo.width, logo.height) : 6;

  // Business phone / delivery number, right under the logo, as a proper
  // label-left/value-right table row (not centered prose) — only drawn at
  // all if at least one is set.
  const contactRows: Array<[string, string]> = [];
  if (config.phone) contactRows.push(["Business No", config.phone]);
  if (config.deliveryNumber) contactRows.push(["Delivery No", config.deliveryNumber]);
  if (contactRows.length > 0) {
    y = drawKeyValueRows(doc, y + 1, contactRows, 8);
    y += 3; // a clear gap before the sale info below, not butted right up against it
  }

  y = drawHeader(doc, null, [`Sale #${sale.id}`, sale.createdAt], y);

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
      // "0.77 kg" for a sold-by-amount line, plain "2" otherwise — the PDF
      // has real proportional layout room for a unit label the fixed
      // 42-character thermal grid doesn't (see `printer::layout::
      // format_qty`'s doc comment on the Rust side).
      formatQty(line.qty, line.unit),
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

function estimatedHeightMm(sale: Sale, config: AppConfig, hasLogo: boolean): number {
  const contactRowCount = (config.phone ? 1 : 0) + (config.deliveryNumber ? 1 : 0);
  // logo + contact rows (+ their gap) + header block + info rows + one row
  // per item + totals block + footer.
  return (
    44 +
    (hasLogo ? LOGO_MAX_HEIGHT_MM + 3 : 0) +
    (contactRowCount > 0 ? contactRowCount * 5 + 4 : 0) +
    10 +
    sale.items.length * 6 +
    36 +
    (config.receiptFooter.trim() ? 10 : 0)
  );
}

/**
 * Builds a compact 80mm-wide receipt PDF, one page. Returns the `jsPDF`
 * document; call `.save(name)` to trigger a download or `.output(...)` for
 * other uses (e.g. a preview).
 *
 * `tablesEnabled` decides the label shown when `sale.tableName` is unset:
 * "Takeaway" if the shop uses tables at all (this sale just wasn't linked to
 * one), "Counter Sale" if the `tables` module isn't in use for this
 * installation — mirrors the same fallback on the ESC/POS side.
 */
export async function buildReceiptPdf(sale: Sale, config: AppConfig, tablesEnabled: boolean): Promise<jsPDF> {
  const logo = await resolveLogo(config.logoPath);
  const doc = newReceiptDoc(estimatedHeightMm(sale, config, logo !== null));
  await drawReceiptContent(doc, sale, config, tablesEnabled, logo);
  return doc;
}

/** Builds and saves the receipt PDF — the on-demand fallback for when
 * there's no thermal printer, or the cashier just wants a PDF copy; see
 * `ReceiptModal.tsx`'s "Save as PDF" button. Never fires automatically. */
export async function downloadReceiptPdf(sale: Sale, config: AppConfig, tablesEnabled: boolean): Promise<boolean> {
  const doc = await buildReceiptPdf(sale, config, tablesEnabled);
  return downloadPdf(doc, `receipt-${sale.id}.pdf`);
}
