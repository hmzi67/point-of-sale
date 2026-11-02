import type { AppConfig, Sale } from "../types";
import { ORDER_TYPE_LABELS, THIRD_PARTY_FALLBACK_LABEL } from "./orderType";

// The receipt's wording and number formats, shared by the PDF (`receiptPdf.ts`),
// the on-screen receipt (`ReceiptPaper`) and — mirrored in Rust — the thermal
// print (`printer::escpos`). Its own module so the receipt popup can use it
// without loading jsPDF.

/** "1,250" for whole amounts, "1,250.50" otherwise — like
 * `printer::escpos::amount`, the slip prints no currency per line. */
export function receiptAmount(minor: number): string {
  const sign = minor < 0 ? "-" : "";
  const abs = Math.abs(minor);
  const whole = Math.floor(abs / 100).toLocaleString("en-US");
  const cents = abs % 100;
  return cents === 0 ? `${sign}${whole}` : `${sign}${whole}.${String(cents).padStart(2, "0")}`;
}

/** "17/10/2026 02:25 AM" from the stored "2026-10-17 02:25:00". */
export function receiptDate(createdAt: string): string {
  const m = /^(\d{4})-(\d{2})-(\d{2}) (\d{2}):(\d{2})/.exec(createdAt);
  if (!m) return createdAt;
  const hour = Number(m[4]);
  const h12 = String(hour % 12 === 0 ? 12 : hour % 12).padStart(2, "0");
  return `${m[3]}/${m[2]}/${m[1]} ${h12}:${m[5]} ${hour < 12 ? "AM" : "PM"}`;
}

/** The "Type :" value — must match `printer::escpos::order_type_label`. */
export function orderTypeLabel(
  sale: Pick<Sale, "tableName" | "orderType" | "deliveryPlatformName">,
  tablesEnabled: boolean,
): string {
  switch (sale.orderType) {
    case "dine_in":
      return "Dine In";
    case "takeaway":
      return ORDER_TYPE_LABELS.takeaway;
    case "delivery":
      return ORDER_TYPE_LABELS.delivery;
    case "third_party_delivery":
      return sale.deliveryPlatformName ?? THIRD_PARTY_FALLBACK_LABEL;
  }
  if (sale.tableName) return "Dine In";
  return tablesEnabled ? ORDER_TYPE_LABELS.takeaway : "Counter Sale";
}

/** "Name (phone)", or just the name when there's no phone. */
export function riderText(name: string, phone?: string | null): string {
  return phone ? `${name} (${phone})` : name;
}

/** The diagonal stamp on a receipt: "PAID" for a settled sale, "UNPAID" while
 * part of it is on pay-later, none when the shop turned stamps off. Must match
 * `printer::escpos::receipt_stamp`. */
export function receiptStamp(
  sale: Pick<Sale, "payments">,
  config: Pick<AppConfig, "receiptShowStamp">,
): "PAID" | "UNPAID" | null {
  if (!config.receiptShowStamp) return null;
  return sale.payments.some((part) => part.method === "credit") ? "UNPAID" : "PAID";
}
