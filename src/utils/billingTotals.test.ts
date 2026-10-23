import { describe, expect, it } from "vitest";
import type { CartEntry } from "../store/billingStore";
import { computeCartTotals, lineTotalMinor } from "./billingTotals";

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
    expect(totals).toEqual({ subtotalMinor: 21000, discountMinor: 0, taxMinor: 0, taxableMinor: 21000, serviceChargeMinor: 0, deliveryChargeMinor: 0, totalMinor: 21000 });
  });

  it("rounds each line like the Rust side, so a fractional qty never leaves a fraction", () => {
    // 1234 × 0.333 = 410.922 -> 411 per line, the same `round()` create_sale applies.
    const totals = computeCartTotals([line(1234, 0.333)], "flat", 0, 0);
    expect(totals.subtotalMinor).toBe(411);
    expect(Number.isInteger(totals.totalMinor)).toBe(true);
  });

  it("adds a service charge on the discounted subtotal, alongside tax", () => {
    // 10000 - 10% = 9000 taxable; 5% tax = 450; 10% service = 900.
    const totals = computeCartTotals([line(10000, 1)], "percent", 10, 5, 0, 10);
    expect(totals.serviceChargeMinor).toBe(900);
    expect(totals.totalMinor).toBe(9000 + 450 + 900);
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

describe("lineTotalMinor", () => {
  it("rounds a fractional qty back to whole minor units", () => {
    // A weighed line: 0.33 kg at Rs 300.00. The raw product is 9900.000000000002.
    expect(lineTotalMinor(30000, 0.33)).toBe(9900);
    // Half a 101-paisa item would be 50.5 — never sent as minor units.
    expect(Number.isInteger(lineTotalMinor(101, 0.5))).toBe(true);
    expect(lineTotalMinor(8000, 3)).toBe(24000);
  });
});

describe("computeCartTotals edge cases", () => {
  it("adds the delivery fee to the total without taxing it", () => {
    const totals = computeCartTotals([line(10000, 2)], "flat", 0, 10, 2000);
    expect(totals).toMatchObject({ subtotalMinor: 20000, taxMinor: 2000, deliveryChargeMinor: 2000, totalMinor: 24000 });
  });

  it("caps a discount at the subtotal so the total never goes negative", () => {
    expect(computeCartTotals([line(5000, 1)], "flat", 80, 10).totalMinor).toBe(0);
    expect(computeCartTotals([line(5000, 1)], "percent", 150, 10).discountMinor).toBe(5000);
  });

  it("returns all zeros for an empty cart", () => {
    const totals = computeCartTotals([], "percent", 10, 16);
    expect(totals.totalMinor).toBe(0);
    expect(totals.discountMinor).toBe(0);
  });
});

