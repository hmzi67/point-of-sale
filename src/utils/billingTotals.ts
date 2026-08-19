import type { CartEntry, DiscountMode } from "../store/billingStore";
import { decimalToMinor } from "./format";

export interface CartTotals {
  subtotalMinor: number;
  discountMinor: number;
  taxMinor: number;
  /** subtotal - discount, i.e. what tax is calculated on. */
  taxableMinor: number;
  totalMinor: number;
}

/**
 * Pure total calculation shared by the cart panel, the sale submission and
 * the receipt — so the number shown at checkout is guaranteed to be the same
 * number sent to `billing_create_sale`.
 *
 * Mirrors the Rust seed data's tax formula (`round(taxable * percent / 100)`)
 * so a receipt's tax line always matches what the backend would compute for
 * the same inputs.
 */
export function computeCartTotals(
  cart: CartEntry[],
  discountMode: DiscountMode,
  discountValue: number,
  taxPercent: number,
): CartTotals {
  const subtotalMinor = cart.reduce((sum, line) => sum + line.priceMinor * line.qty, 0);

  const rawDiscountMinor =
    discountMode === "percent"
      ? Math.round((subtotalMinor * discountValue) / 100)
      : decimalToMinor(discountValue);
  const discountMinor = Math.max(0, Math.min(rawDiscountMinor, subtotalMinor));

  const taxableMinor = subtotalMinor - discountMinor;
  const taxMinor = Math.round((taxableMinor * taxPercent) / 100);
  const totalMinor = taxableMinor + taxMinor;

  return { subtotalMinor, discountMinor, taxMinor, taxableMinor, totalMinor };
}
