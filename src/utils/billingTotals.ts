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
  // Rounded per line, not once over the sum — this mirrors `db::sales::
  // create_sale`'s `(price_minor as f64 * line.qty).round()` exactly, so the
  // subtotal shown at checkout is the same integer the server derives rather
  // than one that can drift a paisa from it.
  //
  // The rounding also keeps money whole: an amount-entered line carries a
  // full-precision qty (see `billingStore.addItemByAmount`), so the raw
  // product is something like 9999.999999999998, and letting that flow on
  // would put a fractional value into fields that are integer minor units
  // everywhere else.
  const subtotalMinor = cart.reduce((sum, line) => sum + Math.round(line.priceMinor * line.qty), 0);

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
