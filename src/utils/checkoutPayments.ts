import type { CheckoutMode, CollectionMethod, PaymentMethod, PaymentPart, SupplierPaymentMethod } from "../types";
import { decimalToMinor, formatMinor } from "./format";
import { toDateString } from "./dateRanges";

export type SplitMethod = "cash" | "card" | "other" | "credit";

/** The tenders the split editor offers (credit only with customers on). */
export const SPLIT_METHODS: SplitMethod[] = ["cash", "card", "other", "credit"];
/** How a pay-later installment can be received (`customer_payments.method`). */
export const COLLECTION_METHODS: CollectionMethod[] = ["cash", "card", "other"];
/** How a supplier can be paid (`supplier_payments.method`). */
export const SUPPLIER_PAYMENT_METHODS: SupplierPaymentMethod[] = ["cash", "bank", "card", "other"];

/** One row of the split-payment editor; `amount` is the raw decimal the
 * cashier typed. */
export interface SplitRow {
  method: SplitMethod;
  amount: string;
}

export interface CheckoutPayments {
  /** Sent as `paymentMethod`; ignored by the server when `payments` is set. */
  paymentMethod: PaymentMethod;
  payments?: PaymentPart[];
  /** The pay-later part, which needs a customer and a due date. */
  creditMinor: number;
  /** Why the sale can't be placed yet, or `null`. */
  error: string | null;
}

/**
 * Turns the cashier's checkout choice into what `billing_create_sale` takes.
 * `payableMinor` must be the total the server will derive (the cart total
 * without any delivery fee, which lives on the order) — the server refuses
 * tenders that don't add up to it exactly.
 */
export function buildCheckoutPayments(
  mode: CheckoutMode,
  payableMinor: number,
  splitRows: SplitRow[],
  currency: string,
): CheckoutPayments {
  if (mode === "credit") {
    return {
      paymentMethod: "cash",
      payments: payableMinor > 0 ? [{ method: "credit", amountMinor: payableMinor }] : [],
      creditMinor: payableMinor,
      error: null,
    };
  }
  if (mode !== "split") {
    return { paymentMethod: mode, creditMinor: 0, error: null };
  }

  const payments: PaymentPart[] = [];
  for (const row of splitRows) {
    const amount = Number(row.amount);
    if (!row.amount.trim() || !Number.isFinite(amount) || amount <= 0) continue;
    payments.push({ method: row.method, amountMinor: decimalToMinor(amount) });
  }
  const allocated = payments.reduce((sum, part) => sum + part.amountMinor, 0);
  const creditMinor = payments.filter((p) => p.method === "credit").reduce((sum, p) => sum + p.amountMinor, 0);
  let error: string | null = null;
  if (payments.length === 0) error = "Enter how the bill is being paid";
  else if (allocated < payableMinor) error = `${formatMinor(payableMinor - allocated, currency)} still to allocate`;
  else if (allocated > payableMinor) error = `Payments are ${formatMinor(allocated - payableMinor, currency)} over the total`;
  return { paymentMethod: "cash", payments, creditMinor, error };
}

/** The default pay-later due date: `days` from `today`. */
export function defaultDueDate(days = 7, today = new Date()): string {
  const due = new Date(today);
  due.setDate(due.getDate() + days);
  return toDateString(due);
}

const METHOD_LABELS: Record<string, string> = {
  cash: "Cash",
  card: "Card",
  other: "Other",
  platform: "Platform",
  credit: "Pay later",
  split: "Split",
  bank: "Bank",
};

export function paymentMethodLabel(method: string): string {
  return METHOD_LABELS[method] ?? method;
}
