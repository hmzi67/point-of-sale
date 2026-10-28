import type { jsPDF } from "jspdf";
import { formatMinor, formatQty } from "./format";
import { downloadPdf } from "./pdfExport";
import {
  drawDashedRule,
  drawRule,
  drawLogo,
  drawMonoCentered,
  drawMonoRow,
  drawRuledItemTable,
  MARGIN_MM,
  newReceiptDoc,
  trimLogo,
  PAGE_WIDTH_MM,
  THERMAL_LINE_WIDTH,
} from "./printLayout";
import { devWarn } from "../services/logService";
import type { AppConfig, Sale } from "../types";
import { paymentMethodLabel } from "./checkoutPayments";
import { ORDER_TYPE_LABELS, THIRD_PARTY_FALLBACK_LABEL } from "./orderType";

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
    return await trimLogo(dataUrl);
  } catch (error) {
    devWarn("[receiptPdf] logo could not be loaded, printing without it", error);
    return null;
  }
}

/** The developer credit printed at the foot of the receipt — kept in
 * lockstep with `printer::escpos`'s `DEVELOPER_CREDIT` / `DEVELOPER_SITE` /
 * `DEVELOPER_PHONE` / `CREDIT_SEPARATOR` constants on the Rust side, which
 * print the same block on the thermal path. Constants rather than config
 * fields on purpose: this is the vendor's attribution, not a per-client
 * setting. The phone stays spaced rather than one unbroken digit string so
 * it reads as a number a customer could dial off the paper.
 *
 * The trademark is the real `™` here, where the thermal path spells it
 * `(TM)` — that path prints in codepage CP437, which has no trademark
 * glyph at any codepoint, while a PDF has a real font and no such limit.
 * It's the one place the two outputs deliberately differ. */
const DEVELOPER_CREDIT = "Developed by Code Hunts™";
const DEVELOPER_SITE = "codehunts.co.uk";
const DEVELOPER_PHONE = "+92 339 0129547";
const CREDIT_SEPARATOR = "  |  ";

/**
 * Draws the receipt's full content onto whatever page `doc` is currently
 * on — the one place that content is actually laid out, mirroring
 * `printer::escpos::build_receipt_bytes` on the ESC/POS side section for
 * section: logo, shop name and numbers, sale info, bordered item table,
 * totals, payment line, thank-you, developer credit.
 *
 * Everything below the logo is drawn in the monospaced face
 * (`printLayout.ts`'s `MONO_FONT`) with horizontal rules and no vertical
 * borders, so the PDF reads as the same document the thermal printer
 * produces rather than a separately-designed one.
 *
 * The logo is the receipt's brand mark (see `printLayout.ts`'s
 * `LOGO_MAX_WIDTH_MM`); the business name stands in only when there is none.
 */
function drawReceiptContent(
  doc: jsPDF,
  sale: Sale,
  config: AppConfig,
  tablesEnabled: boolean,
  logo: { dataUrl: string; width: number; height: number } | null,
): number {
  // Masthead: the logo (no shop name — the logo is the brand mark), then
  // address, website and numbers as small centred lines.
  let y = logo ? drawLogo(doc, logo.dataUrl, logo.width, logo.height, 2) + 0.5 : 2;
  doc.setFont("courier", "normal");
  doc.setFontSize(8);
  const contentWidth = PAGE_WIDTH_MM - MARGIN_MM * 2;
  const numbers = [config.phone && `Tel: ${config.phone}`, config.deliveryNumber && `Delivery: ${config.deliveryNumber}`].filter(Boolean) as string[];
  const lines = [
    ...(config.businessAddress?.trim() ? (doc.splitTextToSize(config.businessAddress.trim(), contentWidth) as string[]).slice(0, 2) : []),
    ...(config.businessWebsite?.trim() ? [config.businessWebsite.trim()] : []),
    ...(numbers.join("  |  ").length <= THERMAL_LINE_WIDTH ? [numbers.join("  |  ")].filter(Boolean) : numbers),
  ];
  for (const line of lines) {
    drawMonoCentered(doc, y + 1, line, { size: 8 });
    y += 3.8;
  }
  y = drawRule(doc, y + 0.5) + 1;

  // Sale identity: number + timestamp on one line, cashier + order type on
  // the next — the same two-line data block the thermal path prints.
  y = drawMonoRow(doc, y, `Sale #${sale.receiptNo}`, sale.createdAt);
  if (sale.tillName) y = drawMonoRow(doc, y, `Counter: ${sale.tillName}`, "");
  y = drawMonoRow(doc, y, `Cashier: ${sale.cashierName ?? "—"}`, receiptOrderField(sale, tablesEnabled));
  y -= 1.5;

  if (sale.orderType === "delivery") {
    if (sale.customerPhone || sale.customerName) {
      const custVal = `${sale.customerName ?? ""}${sale.customerPhone ? ` (${sale.customerPhone})` : ""}`.trim();
      y = drawMonoRow(doc, y, "Customer:", custVal);
    }
    if (sale.deliveryAddress) {
      doc.setFont("courier", "normal");
      doc.setFontSize(8);
      const splitAddr = doc.splitTextToSize(`Address: ${sale.deliveryAddress}`, 68);
      doc.text(splitAddr, MARGIN_MM, y);
      y += splitAddr.length * 3.5;
    }
    if (sale.deliveryRiderName) {
      y = drawMonoRow(doc, y, "Rider:", riderText(sale.deliveryRiderName, sale.deliveryRiderPhone));
    }
    y += 1;
  } else if (sale.orderType === "third_party_delivery") {
    if (sale.platformOrderRef) {
      y = drawMonoRow(doc, y, "Platform Ref:", `#${sale.platformOrderRef}`);
    }
    if (sale.deliveryRiderName) {
      y = drawMonoRow(doc, y, "Rider:", riderText(sale.deliveryRiderName, sale.deliveryRiderPhone));
    }
    y += 1;
  }

  y = drawRuledItemTable(
    doc,
    y,
    sale.items.map((line) => [
      line.itemName,
      // "0.77 kg" for a sold-by-amount line, plain "2" otherwise — the PDF
      // has real proportional layout room for a unit label the fixed
      // 42-character thermal grid doesn't (see `printer::layout::
      // format_qty`'s doc comment on the Rust side).
      formatQty(line.qty, line.unit),
      formatMinor(line.priceAtSaleMinor, "").trim(),
      formatMinor(line.lineTotalMinor, "").trim(),
    ]),
  );

  y = drawMonoRow(doc, y, "Subtotal:", formatMinor(sale.subtotalMinor, config.currency));
  if (sale.discountMinor > 0) {
    y = drawMonoRow(doc, y, "Discount:", `-${formatMinor(sale.discountMinor, config.currency)}`);
  }
  if (sale.taxMinor > 0) {
    y = drawMonoRow(doc, y, "Tax:", formatMinor(sale.taxMinor, config.currency));
  }
  if (sale.serviceChargeMinor > 0) {
    y = drawMonoRow(doc, y, "Service Charge:", formatMinor(sale.serviceChargeMinor, config.currency));
  }
  if (sale.deliveryChargeMinor && sale.deliveryChargeMinor > 0) {
    y = drawMonoRow(doc, y, "Delivery Fee:", formatMinor(sale.deliveryChargeMinor, config.currency));
  }
  // TOTAL: the one big line on the slip, between two rules — plain black,
  // which reads cleanly on thermal paper where a filled band smears.
  y = drawRule(doc, y - 2.4);
  y = drawMonoRow(doc, y + 2.2, "TOTAL", formatMinor(sale.totalMinor, config.currency), { bold: true, size: 13 });
  y = drawRule(doc, y - 2.2) + 0.5;

  y = drawMonoRow(doc, y, "Paid by:", paymentMethodLabel(sale.paymentMethod), { bold: true });
  if (hasTenderBreakdown(sale)) {
    for (const part of sale.payments) {
      y = drawMonoRow(doc, y, `${paymentMethodLabel(part.method)}:`, formatMinor(part.amountMinor, config.currency));
    }
    if (sale.dueDate) y = drawMonoRow(doc, y, "Balance due by:", sale.dueDate, { bold: true });
  }
  if (sale.payments.some((part) => part.method === "cash") && sale.tenderedMinor != null) {
    const change = sale.changeMinor ?? Math.max(0, sale.tenderedMinor - sale.totalMinor);
    y = drawMonoRow(doc, y, "Cash Received:", formatMinor(sale.tenderedMinor, config.currency));
    y = drawMonoRow(doc, y, "Change to Return:", formatMinor(change, config.currency), { bold: true });
    y += 3;
  } else {
    y += 3;
  }

  if (config.receiptFooter.trim()) {
    y = drawMonoCentered(doc, y, config.receiptFooter.trim(), { bold: true, size: 9 });
  }

  // A dashed rule closes off the transaction and opens the credit block —
  // still lighter than the solid rules bracketing the totals, so it reads
  // as a change of subject rather than a structural boundary.
  y = drawDashedRule(doc, y - 1.5);
  y += 2.5;

  // Smaller and lighter than the transaction above it, so it reads as a
  // footer rather than competing with the receipt's own content. The
  // site/phone line splits in two exactly when the thermal path's does —
  // both derive the decision from the same character budget, so the two
  // outputs never disagree about which shape the block takes.
  y = drawMonoCentered(doc, y, DEVELOPER_CREDIT, { size: 7.5, muted: true });
  const combined = `${DEVELOPER_SITE}${CREDIT_SEPARATOR}${DEVELOPER_PHONE}`;
  if (combined.length <= THERMAL_LINE_WIDTH) {
    y = drawMonoCentered(doc, y, combined, { size: 7, muted: true });
  } else {
    y = drawMonoCentered(doc, y, DEVELOPER_SITE, { size: 7, muted: true });
    y = drawMonoCentered(doc, y, DEVELOPER_PHONE, { size: 7, muted: true });
  }
  return y;
}

/** Several tenders, or a pay-later sale — the receipt lists each part. */
function hasTenderBreakdown(sale: Sale): boolean {
  return sale.payments.length > 1 || sale.paymentMethod === "credit";
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
/**
 * The "Table: …" / "Type: …" field beside the cashier. Must match the thermal
 * receipt (`printer::escpos::build_receipt_bytes`) exactly; an order with no
 * known type is a counter sale, called "Takeaway" when tables are in use.
 */
export function receiptOrderField(
  sale: Pick<Sale, "tableName" | "orderType" | "deliveryPlatformName" | "orderNo">,
  tablesEnabled: boolean,
): string {
  let label = "Type";
  let value: string;
  if (sale.tableName) {
    label = "Table";
    value = sale.tableName;
  } else if (sale.orderType === "third_party_delivery") value = sale.deliveryPlatformName ?? THIRD_PARTY_FALLBACK_LABEL;
  else if (sale.orderType === "delivery" || sale.orderType === "takeaway" || sale.orderType === "dine_in") {
    value = ORDER_TYPE_LABELS[sale.orderType];
  } else value = tablesEnabled ? ORDER_TYPE_LABELS.takeaway : "Counter Sale";
  return sale.orderNo ? `${label}: ${value} (#${sale.orderNo})` : `${label}: ${value}`;
}

/** "Name (phone)", or just the name when there's no phone. */
export function riderText(name: string, phone?: string | null): string {
  return phone ? `${name} (${phone})` : name;
}

export async function buildReceiptPdf(sale: Sale, config: AppConfig, tablesEnabled: boolean): Promise<jsPDF> {
  const logo = await resolveLogo(config.logoPath);
  // Drawn twice: once on a throwaway long page to measure, then on a page
  // cut to exactly that height, so no blank paper trails the receipt.
  const contentEnd = drawReceiptContent(newReceiptDoc(1000), sale, config, tablesEnabled, logo);
  const doc = newReceiptDoc(contentEnd + 2, 0);
  drawReceiptContent(doc, sale, config, tablesEnabled, logo);
  return doc;
}

/** Builds and saves the receipt PDF — the on-demand fallback for when
 * there's no thermal printer, or the cashier just wants a PDF copy; see
 * `ReceiptModal.tsx`'s "Save as PDF" button. Never fires automatically. */
export async function downloadReceiptPdf(sale: Sale, config: AppConfig, tablesEnabled: boolean): Promise<boolean> {
  const doc = await buildReceiptPdf(sale, config, tablesEnabled);
  return downloadPdf(doc, `receipt-${sale.id}.pdf`);
}
