import { jsPDF } from "jspdf";
import arimoMedium from "../assets/fonts/Arimo-Medium.ttf?inline";
import arimoBold from "../assets/fonts/Arimo-Bold.ttf?inline";
import { formatQty } from "./format";
import { downloadPdf } from "./pdfExport";
import { trimLogo } from "./printLayout";
import { devWarn } from "../services/logService";
import type { AppConfig, Sale } from "../types";
import { paymentMethodLabel } from "./checkoutPayments";
import { orderTypeLabel, receiptAmount, receiptDate, receiptStamp, riderText } from "./receiptFormat";

type Logo = { dataUrl: string; width: number; height: number };

/** Resolves `config.logoPath` to a cropped, black-and-white logo (see
 * `trimLogo`), or `null` if there's no logo set, the file is missing, or it
 * fails to load — the receipt then simply prints without one, like the
 * thermal path. */
async function resolveLogo(logoPath: string | null): Promise<Logo | null> {
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

/** The developer credit at the foot of the receipt (when
 * `config.receiptShowCredit`) — in lockstep with `printer::escpos`'s
 * `DEVELOPER_CREDIT_FULL` / `DEVELOPER_SITE` / `DEVELOPER_PHONE`. */
const DEVELOPER_CREDIT = "Developed by Code Hunts™";
const DEVELOPER_SITE = "codehunts.co.uk";
const DEVELOPER_PHONE = "+92 339 0129547";
const CREDIT_SEPARATOR = "  |  ";

/** The roll the receipt is laid out for. A 58 mm roll is `{ widthMm: 58,
 * marginMm: 3 }`; everything below is laid out from these two numbers. */
export interface ReceiptPaper {
  widthMm: number;
  marginMm: number;
}
export const PAPER_80MM: ReceiptPaper = { widthMm: 80, marginMm: 4 };

/** Arimo (Arial's metrics, SIL OFL — `src/assets/fonts/Arimo-OFL.txt`),
 * embedded so the PDF looks the same in every viewer and on every printer;
 * the same face the thermal receipt is drawn in. */
const FONT = "Arimo";
/** Point sizes: nothing on the receipt is smaller than 9 pt. */
const SIZE_TITLE = 13.5;
const SIZE_BODY = 10;
const SIZE_TOTAL = 10.5;
const SIZE_SMALL = 9;
const PT_TO_MM = 0.3528;
const LINE_SPACING = 1.55;
/** Logo box and rule weights, in mm. */
const LOGO_MAX_WIDTH_RATIO = 0.5;
const LOGO_MAX_HEIGHT_MM = 22;
const THIN_RULE_MM = 0.18;
const THICK_RULE_MM = 0.4;

/** The stamp: bold, up to 120 pt, turned 38° counter-clockwise, filled with
 * a dot screen of 25% black (like an inked PAID stamp), behind every line of text. */
const STAMP_MAX_PT = 120;
const STAMP_ANGLE_DEG = 38;
const STAMP_DOT_MM = 0.3;
const STAMP_PITCH_MM = 0.6;

/** Item columns, as fractions of the width: Qty / Rate / Disc / Amount —
 * the slip's alignment: Qty from the left, Rate and Amount flush right
 * under their headers, Disc centred. */
const COLUMNS: Array<[number, number, "left" | "center" | "right"]> = [
  [0, 0.2, "left"],
  [0.2, 0.46, "right"],
  [0.52, 0.7, "center"],
  [0.72, 1, "right"],
];

function newDoc(paper: ReceiptPaper, heightMm: number): jsPDF {
  const doc = new jsPDF({ unit: "mm", format: [paper.widthMm, heightMm], compress: true });
  const payload = (dataUrl: string) => dataUrl.slice(dataUrl.indexOf(",") + 1);
  doc.addFileToVFS("Arimo-Medium.ttf", payload(arimoMedium));
  doc.addFont("Arimo-Medium.ttf", FONT, "normal");
  doc.addFileToVFS("Arimo-Bold.ttf", payload(arimoBold));
  doc.addFont("Arimo-Bold.ttf", FONT, "bold");
  doc.setTextColor(0, 0, 0);
  doc.setDrawColor(0, 0, 0);
  return doc;
}

/** Where the drawing ended, and the item-and-totals band the stamp covers
 * (from the rule under the item header to the rule under the totals). */
interface Drawn {
  endMm: number;
  stampTop: number;
  stampBottom: number;
}

/**
 * Draws the receipt onto `doc` — the same rows, in the same order, as
 * `printer::escpos::receipt_rows` prints on paper: logo over a thin rule;
 * shop name, address, phone, website and the tagline banner; the sale's
 * details; Qty / Rate / Disc / Amount with each item's name on its own line
 * and a dotted rule between items; totals and payments between thick rules;
 * the item count, thank-you and follow-us lines, and the developer credit.
 */
function drawReceiptContent(
  doc: jsPDF,
  sale: Sale,
  config: AppConfig,
  tablesEnabled: boolean,
  logo: Logo | null,
  paper: ReceiptPaper,
): Drawn {
  const left = paper.marginMm;
  const right = paper.widthMm - paper.marginMm;
  const span = right - left;
  const middle = paper.widthMm / 2;
  const lineMm = (size: number) => size * PT_TO_MM * LINE_SPACING;
  let y = 3;
  const thickRules: number[] = [];

  const font = (size: number, bold: boolean) => {
    doc.setFont(FONT, bold ? "bold" : "normal");
    doc.setFontSize(size);
  };
  /** One line whose top is `y`. */
  const write = (text: string, x: number, align: "left" | "center" | "right", size: number, bold: boolean) => {
    font(size, bold);
    doc.text(text, x, y + size * PT_TO_MM * 1.02, { align });
  };
  const fit = (text: string, width: number) => {
    if (doc.getTextWidth(text) <= width) return text;
    let cut = text;
    while (cut && doc.getTextWidth(`${cut}…`) > width) cut = cut.slice(0, -1);
    return `${cut.trimEnd()}…`;
  };
  const wrapped = (text: string, align: "left" | "center", size: number, bold: boolean) => {
    font(size, bold);
    for (const line of doc.splitTextToSize(text, span) as string[]) {
      write(line, align === "center" ? middle : left, align, size, bold);
      y += lineMm(size);
    }
  };
  /** Bold label, regular value beside it. */
  /** Bold label, regular value right after it ("Date #:  17/10/2026"). */
  const info = (label: string, value: string) => {
    write(label, left, "left", SIZE_BODY, true);
    const valueX = left + doc.getTextWidth(label) + 2.2;
    font(SIZE_BODY, false);
    write(fit(value, right - valueX), valueX, "left", SIZE_BODY, false);
    y += lineMm(SIZE_BODY);
  };
  /** Bold label left, its amount on the right edge. */
  const total = (label: string, value: string) => {
    write(label, left, "left", SIZE_TOTAL, true);
    font(SIZE_TOTAL, false);
    write(fit(value, span * 0.45), right, "right", SIZE_TOTAL, false);
    y += lineMm(SIZE_TOTAL);
  };
  const columns = (cells: string[], bold: boolean) => {
    cells.forEach((text, i) => {
      const [from, to, align] = COLUMNS[i];
      const x = align === "left" ? left + from * span : align === "right" ? left + to * span : left + ((from + to) / 2) * span;
      font(SIZE_BODY, bold);
      write(fit(text, (to - from) * span - 1), x, align, SIZE_BODY, bold);
    });
    y += lineMm(SIZE_BODY);
  };
  /** `width`: a shorter centred line (the one under the logo). */
  const rule = (kind: "thin" | "thick" | "dotted", width = span) => {
    const mid = y + 1.4;
    doc.setLineWidth(kind === "thick" ? THICK_RULE_MM : THIN_RULE_MM);
    if (kind === "dotted") doc.setLineDashPattern([0.25, 0.8], 0);
    doc.line(middle - width / 2, mid, middle + width / 2, mid);
    doc.setLineDashPattern([], 0);
    if (kind === "thick") thickRules.push(mid);
    y += 3;
  };
  const banner = (text: string) => {
    font(SIZE_BODY, false);
    const shown = fit(text, span * 0.7);
    const width = doc.getTextWidth(shown);
    write(shown, middle, "center", SIZE_BODY, false);
    const mid = y + lineMm(SIZE_BODY) / 2 - 0.2;
    doc.setLineWidth(0.22);
    doc.setLineDashPattern([1, 0.8], 0);
    doc.line(left + 2, mid, middle - width / 2 - 1.5, mid);
    doc.line(middle + width / 2 + 1.5, mid, right - 2, mid);
    doc.setLineDashPattern([], 0);
    y += lineMm(SIZE_BODY);
  };

  if (logo && !logo.dataUrl.startsWith("data:image/svg") && logo.width > 0 && logo.height > 0) {
    // Never stretched: scaled to fit the box, aspect ratio kept.
    const scale = Math.min((paper.widthMm * LOGO_MAX_WIDTH_RATIO) / logo.width, LOGO_MAX_HEIGHT_MM / logo.height);
    const [w, h] = [logo.width * scale, logo.height * scale];
    const format = logo.dataUrl.startsWith("data:image/jpeg") ? "JPEG" : "PNG";
    doc.addImage(logo.dataUrl, format, middle - w / 2, y, w, h, undefined, "NONE");
    y += h + 0.6;
    rule("thin", Math.min(span, w + 6));
  }

  wrapped(config.businessName.toUpperCase(), "center", SIZE_TITLE, true);
  if (config.businessAddress?.trim()) wrapped(config.businessAddress.trim(), "center", SIZE_BODY, false);
  if (config.phone?.trim()) wrapped(`Phone No ${config.phone.trim()}`, "center", SIZE_BODY, false);
  if (config.deliveryNumber?.trim()) wrapped(`Delivery No ${config.deliveryNumber.trim()}`, "center", SIZE_BODY, false);
  if (config.businessWebsite?.trim()) wrapped(config.businessWebsite.trim(), "center", SIZE_BODY, false);
  if (config.receiptTagline?.trim()) banner(config.receiptTagline.trim());
  y += 2.5;

  info("Cashier:", (sale.cashierName ?? "—").toUpperCase());
  info("Date #:", receiptDate(sale.createdAt));
  info("Invoice #:", sale.receiptNo);
  if (sale.orderNo) info("Order #:", String(sale.orderNo));
  if (sale.tillName) info("Counter:", sale.tillName);
  info("Type :", orderTypeLabel(sale, tablesEnabled));
  if (sale.tableName) info("Table #:", sale.tableName);
  if (sale.orderType === "delivery") {
    if (sale.customerName || sale.customerPhone) info("Customer:", riderText(sale.customerName ?? "", sale.customerPhone).trim());
    if (sale.deliveryAddress?.trim()) wrapped(`Address: ${sale.deliveryAddress.trim()}`, "left", SIZE_BODY, false);
    if (sale.deliveryRiderName) info("Rider:", riderText(sale.deliveryRiderName, sale.deliveryRiderPhone));
  } else if (sale.orderType === "third_party_delivery") {
    if (sale.platformOrderRef) info("Order Ref:", `#${sale.platformOrderRef}`);
    if (sale.deliveryRiderName) info("Rider:", riderText(sale.deliveryRiderName, sale.deliveryRiderPhone));
  }
  y += 2.5;

  columns(["Qty", "Rate", "Disc", "Amount"], true);
  rule("thick");
  sale.items.forEach((line, i) => {
    if (i > 0) rule("dotted");
    wrapped(line.itemName, "left", SIZE_BODY, true);
    if (line.notes?.trim()) wrapped(`(${line.notes.trim()})`, "left", SIZE_SMALL, false);
    // The discount is taken off the whole bill, never per line.
    columns([formatQty(line.qty, line.unit), receiptAmount(line.priceAtSaleMinor), "0", receiptAmount(line.lineTotalMinor)], false);
  });
  rule("thick");

  total("SubTotal", receiptAmount(sale.subtotalMinor));
  if (sale.discountMinor > 0) total("Discount", `-${receiptAmount(sale.discountMinor)}`);
  if (sale.taxMinor > 0) total("Tax", receiptAmount(sale.taxMinor));
  if (sale.serviceChargeMinor > 0) total("Service Charge", receiptAmount(sale.serviceChargeMinor));
  if (sale.deliveryChargeMinor && sale.deliveryChargeMinor > 0) total("Delivery Fee", receiptAmount(sale.deliveryChargeMinor));
  total("NetTotal", receiptAmount(sale.totalMinor));
  const tendered = sale.payments.some((part) => part.method === "cash") ? sale.tenderedMinor ?? null : null;
  for (const part of sale.payments) {
    // Cash shows what the customer handed over; the change row follows.
    const paid = part.method === "cash" && tendered !== null ? tendered : part.amountMinor;
    total(`Payment (${paymentMethodLabel(part.method)})`, receiptAmount(paid));
  }
  if (sale.dueDate) total("Balance Due By", sale.dueDate);
  if (tendered !== null) total("Change", receiptAmount(sale.changeMinor ?? Math.max(0, tendered - sale.totalMinor)));
  rule("thick");
  y += 3.5;

  const totalQty = sale.items.reduce((sum, line) => sum + line.qty, 0);
  wrapped(`Total Items: ${sale.items.length} Total QTY: ${formatQty(totalQty, null)}`, "center", SIZE_BODY, true);
  for (const line of config.receiptFooter.split("\n").map((l) => l.trim()).filter(Boolean)) {
    wrapped(line, "center", SIZE_BODY, false);
  }
  if (config.receiptSocial?.trim()) wrapped(config.receiptSocial.trim(), "center", SIZE_SMALL, false);
  if (config.receiptShowCredit) {
    y += 3;
    wrapped(DEVELOPER_CREDIT, "center", SIZE_SMALL, false);
    wrapped(`${DEVELOPER_SITE}${CREDIT_SEPARATOR}${DEVELOPER_PHONE}`, "center", SIZE_SMALL, false);
  }
  return { endMm: y, stampTop: thickRules[0] ?? 0, stampBottom: thickRules[thickRules.length - 1] ?? 0 };
}

/**
 * The stamp as real text used as a clipping path, filled with a dot screen,
 * so it reads like an ink stamp and stays vector. Drawn before the receipt,
 * so every line of text lands on top of it, fully black.
 */
function drawStamp(doc: jsPDF, text: string, top: number, bottom: number, paper: ReceiptPaper) {
  if (bottom <= top) return;
  const span = paper.widthMm - paper.marginMm * 2;
  const rad = (STAMP_ANGLE_DEG * Math.PI) / 180;
  const [sin, cos] = [Math.sin(rad), Math.cos(rad)];
  doc.setFont(FONT, "bold");
  const perPt = (doc.getStringUnitWidth(text) * PT_TO_MM);
  const capPerPt = 0.72 * PT_TO_MM;
  const size = Math.min(
    STAMP_MAX_PT,
    span / (perPt * cos + capPerPt * sin),
    ((bottom - top) * 1.15) / (perPt * sin + capPerPt * cos),
  );
  const [w, h] = [perPt * size, capPerPt * size];
  const [cx, cy] = [paper.widthMm / 2, (top + bottom) / 2];
  // The baseline's start, so the turned word is centred on (cx, cy).
  const x = cx - (cos * w) / 2 + (sin * h) / 2;
  const y = cy + (sin * w) / 2 + (cos * h) / 2;

  doc.saveGraphicsState();
  doc.setFontSize(size);
  doc.text(text, x, y, { angle: STAMP_ANGLE_DEG, renderingMode: "addToPathForClipping" });
  doc.setFillColor(0, 0, 0);
  const reach = Math.hypot(w, h) / 2 + 1;
  for (let dy = cy - reach; dy < cy + reach; dy += STAMP_PITCH_MM) {
    for (let dx = Math.max(0, cx - reach); dx < Math.min(paper.widthMm, cx + reach); dx += STAMP_PITCH_MM) {
      doc.rect(dx, dy, STAMP_DOT_MM, STAMP_DOT_MM, "F");
    }
  }
  doc.restoreGraphicsState();
}

/**
 * Builds the receipt PDF: one page exactly as tall as its content, at
 * `paper`'s width. Returns the `jsPDF` document; call `.save(name)` or
 * `.output(...)` on it.
 *
 * `tablesEnabled` decides the "Type" of a sale with no recorded order type:
 * "Takeaway" if the shop uses tables, "Counter Sale" if not — like the
 * thermal path.
 */
export async function buildReceiptPdf(
  sale: Sale,
  config: AppConfig,
  tablesEnabled: boolean,
  paper: ReceiptPaper = PAPER_80MM,
): Promise<jsPDF> {
  const logo = await resolveLogo(config.logoPath);
  // Drawn twice: once on a throwaway long page to measure (and to find the
  // stamp's band), then on a page cut to exactly that height.
  const measured = drawReceiptContent(newDoc(paper, 5000), sale, config, tablesEnabled, logo, paper);
  const doc = newDoc(paper, measured.endMm + 3);
  const stamp = receiptStamp(sale, config);
  if (stamp) drawStamp(doc, stamp, measured.stampTop, measured.stampBottom, paper);
  drawReceiptContent(doc, sale, config, tablesEnabled, logo, paper);
  return doc;
}

/** Builds and saves the receipt PDF — the on-demand fallback for when
 * there's no thermal printer, or the cashier just wants a PDF copy; see
 * `ReceiptModal.tsx`'s "Save as PDF" button. Never fires automatically. */
export async function downloadReceiptPdf(sale: Sale, config: AppConfig, tablesEnabled: boolean): Promise<boolean> {
  const doc = await buildReceiptPdf(sale, config, tablesEnabled);
  return downloadPdf(doc, `receipt-${sale.id}.pdf`);
}
