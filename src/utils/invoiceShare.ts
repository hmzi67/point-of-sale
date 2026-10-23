import type { AppConfig, Sale } from "../types";
import { paymentMethodLabel } from "./checkoutPayments";
import { formatMinor, formatQty } from "./format";

/** A plain-text invoice for WhatsApp/email — the same facts as the printed
 * receipt, in a form a chat message or email body can carry. */
export function buildInvoiceText(sale: Sale, config: AppConfig): string {
  const money = (minor: number) => formatMinor(minor, config.currency);
  const lines: string[] = [config.businessName, `Invoice #${sale.id} — ${sale.createdAt}`];
  if (sale.customerName) lines.push(`Customer: ${sale.customerName}`);
  lines.push("");
  for (const item of sale.items) {
    lines.push(`${item.itemName} × ${formatQty(item.qty, item.unit)} = ${money(item.lineTotalMinor)}`);
  }
  lines.push("", `Subtotal: ${money(sale.subtotalMinor)}`);
  if (sale.discountMinor > 0) lines.push(`Discount: -${money(sale.discountMinor)}`);
  if (sale.taxMinor > 0) lines.push(`Tax: ${money(sale.taxMinor)}`);
  if (sale.serviceChargeMinor > 0) lines.push(`Service charge: ${money(sale.serviceChargeMinor)}`);
  lines.push(`Total: ${money(sale.totalMinor)}`);
  for (const part of sale.payments) {
    lines.push(`${paymentMethodLabel(part.method)}: ${money(part.amountMinor)}`);
  }
  if (sale.dueDate) lines.push(`Balance due by ${sale.dueDate}`);
  if (config.receiptFooter) lines.push("", config.receiptFooter);
  return lines.join("\n");
}

/** wa.me wants an international number with no "+" or leading zeros. A
 * local Pakistani mobile (03xx…) becomes 923xx…; anything already
 * international is passed through as digits. */
export function whatsappNumber(phone: string | null | undefined, currency: string): string {
  if (!phone) return "";
  const trimmed = phone.trim();
  let digits = trimmed.replace(/\D/g, "");
  if (digits.startsWith("00")) digits = digits.slice(2);
  else if (!trimmed.startsWith("+") && digits.startsWith("0") && currency === "PKR") digits = `92${digits.slice(1)}`;
  return digits;
}

export function whatsappUrl(phone: string | null | undefined, currency: string, text: string): string {
  return `https://wa.me/${whatsappNumber(phone, currency)}?text=${encodeURIComponent(text)}`;
}

export function mailtoUrl(email: string | null | undefined, subject: string, body: string): string {
  return `mailto:${encodeURIComponent(email ?? "")}?subject=${encodeURIComponent(subject)}&body=${encodeURIComponent(body)}`;
}
