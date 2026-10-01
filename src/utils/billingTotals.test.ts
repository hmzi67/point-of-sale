import { describe, expect, it } from "vitest";
import type { CartEntry } from "../store/billingStore";
import { computeCartTotals } from "./billingTotals";

function line(priceMinor: number, qty: number): CartEntry {
  return {
    itemId: priceMinor,
    name: "Item",
    barcode: null,
    priceMinor,
    stockQty: 100,
    qty,
    imagePath: null,
    notes: "",
    soldByAmount: false,
    unit: null,
  };
}

describe("computeCartTotals", () => {
  it("sums lines and returns whole minor units with no discount or tax", () => {
    const totals = computeCartTotals([line(8000, 2), line(5000, 1)], "flat", 0, 0);
    expect(totals).toEqual({ subtotalMinor: 21000, discountMinor: 0, taxMinor: 0, taxableMinor: 21000, totalMinor: 21000 });
  });

  it("rounds each line like the Rust side, so a fractional qty never leaves a fraction", () => {
    // 1234 × 0.333 = 410.922 -> 411 per line, the same `round()` create_sale applies.
    const totals = computeCartTotals([line(1234, 0.333)], "flat", 0, 0);
    expect(totals.subtotalMinor).toBe(411);
    expect(Number.isInteger(totals.totalMinor)).toBe(true);
  });

  it("applies a flat discount entered as a decimal amount", () => {
    expect(computeCartTotals([line(10000, 1)], "flat", 12.5, 0).discountMinor).toBe(1250);
  });

  it("applies a percent discount, rounded to whole minor units", () => {
    expect(computeCartTotals([line(999, 1)], "percent", 10, 0).discountMinor).toBe(100);
  });

  it("never discounts below zero or past the subtotal", () => {
    expect(computeCartTotals([line(1000, 1)], "flat", 50, 0).discountMinor).toBe(1000);
    expect(computeCartTotals([line(1000, 1)], "percent", 150, 0).totalMinor).toBe(0);
    expect(computeCartTotals([line(1000, 1)], "flat", -5, 0).discountMinor).toBe(0);
  });

  it("taxes the discounted amount, not the subtotal", () => {
    const totals = computeCartTotals([line(10000, 1)], "flat", 20, 17);
    expect(totals.taxableMinor).toBe(8000);
    expect(totals.taxMinor).toBe(1360);
    expect(totals.totalMinor).toBe(9360);
  });

  it("returns all zeros for an empty cart", () => {
    expect(computeCartTotals([], "percent", 10, 17).totalMinor).toBe(0);
  });
});
