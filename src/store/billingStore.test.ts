import { beforeEach, describe, expect, it } from "vitest";
import type { Item } from "../types";
import { computeCartTotals } from "../utils/billingTotals";
import { useBillingStore } from "./billingStore";

function item(overrides: Partial<Item> = {}): Item {
  return {
    id: 1,
    name: "Cola",
    barcode: null,
    shortCode: null,
    description: null,
    priceMinor: 8000,
    costMinor: 5000,
    stockQty: 10,
    categoryId: null,
    categoryName: null,
    lowStockThreshold: 0,
    isActive: true,
    isLowStock: false,
    imagePath: null,
    soldByAmount: false,
    unit: null,
    counterId: null,
    counterName: null,
    ...overrides,
  };
}

const loose = (overrides: Partial<Item> = {}) =>
  item({ id: 2, name: "Rice", priceMinor: 30000, stockQty: 50, soldByAmount: true, unit: "kg", ...overrides });

const store = () => useBillingStore.getState();
const qtyOf = (id: number) => store().cart[id]?.qty;

beforeEach(() => {
  store().clearCart();
});

describe("addItem", () => {
  it("adds a per-piece item at 1 and increments on repeat, capped at stock", () => {
    const cola = item({ stockQty: 2 });
    store().addItem(cola);
    expect(qtyOf(1)).toBe(1);
    store().addItem(cola);
    store().addItem(cola);
    expect(qtyOf(1)).toBe(2);
    expect(store().cartOrder).toEqual([1]);
  });

  it("ignores an out-of-stock item", () => {
    store().addItem(item({ stockQty: 0 }));
    expect(store().cartOrder).toEqual([]);
  });

  it("starts a sold-by-amount line at 0 and leaves it alone on repeat adds", () => {
    store().addItem(loose());
    expect(qtyOf(2)).toBe(0);
    store().addItem(loose());
    expect(qtyOf(2)).toBe(0);
  });
});

describe("addItemByAmount", () => {
  it("sets (not adds) the qty, keeping full precision so the bill matches the typed amount", () => {
    store().addItem(loose());
    store().addItemByAmount(loose(), 10000);
    store().addItemByAmount(loose(), 10000);
    expect(qtyOf(2)).toBeCloseTo(1 / 3, 12);

    // 300.00/kg × (100.00 ÷ 300.00) bills exactly 100.00, not 99.00.
    const totals = computeCartTotals([store().cart[2]], "flat", 0, 0);
    expect(totals.subtotalMinor).toBe(10000);
  });

  it("creates the line if it isn't in the cart yet", () => {
    store().addItemByAmount(loose(), 15000);
    expect(store().cartOrder).toEqual([2]);
    expect(qtyOf(2)).toBe(0.5);
  });

  it("is a no-op for a per-piece item, a zero amount or no stock", () => {
    store().addItemByAmount(item(), 10000);
    store().addItemByAmount(loose(), 0);
    store().addItemByAmount(loose({ stockQty: 0 }), 10000);
    expect(store().cartOrder).toEqual([]);
  });

  it("caps the qty at stock", () => {
    store().addItemByAmount(loose({ stockQty: 1 }), 90000);
    expect(qtyOf(2)).toBe(1);
  });
});

describe("setQty / updateLine clamping", () => {
  it("clamps a per-piece line between 1 and stock", () => {
    store().addItem(item({ stockQty: 5 }));
    store().setQty(1, 99);
    expect(qtyOf(1)).toBe(5);
    store().setQty(1, 0);
    expect(qtyOf(1)).toBe(1);
  });

  it("lets a sold-by-amount line go down to 0.01", () => {
    store().addItem(loose());
    store().setQty(2, 0);
    expect(qtyOf(2)).toBe(0.01);
  });

  it("updateLine sets both qty and notes", () => {
    store().addItem(item());
    store().updateLine(1, 3, "no ice");
    expect(store().cart[1]).toMatchObject({ qty: 3, notes: "no ice" });
  });

  it("ignores a line that isn't in the cart", () => {
    store().setQty(42, 3);
    expect(store().cart[42]).toBeUndefined();
  });
});

describe("keyboard active line", () => {
  beforeEach(() => {
    store().addItem(item({ id: 1 }));
    store().addItem(item({ id: 2 }));
    store().addItem(item({ id: 3 }));
  });

  it("picks the first line going down and the last going up when nothing is active", () => {
    store().moveActiveLine(1);
    expect(store().activeLineItemId).toBe(1);
    store().setActiveLine(null);
    store().moveActiveLine(-1);
    expect(store().activeLineItemId).toBe(3);
  });

  it("stops at either end instead of wrapping", () => {
    store().setActiveLine(3);
    store().moveActiveLine(1);
    expect(store().activeLineItemId).toBe(3);
    store().setActiveLine(1);
    store().moveActiveLine(-1);
    expect(store().activeLineItemId).toBe(1);
  });

  it("removes the line when the qty drops below the minimum", () => {
    store().setActiveLine(2);
    store().adjustActiveQty(-1);
    expect(store().cart[2]).toBeUndefined();
    expect(store().cartOrder).toEqual([1, 3]);
    expect(store().activeLineItemId).toBeNull();
  });

  it("caps an increase at stock", () => {
    store().setActiveLine(1);
    for (let i = 0; i < 20; i++) store().adjustActiveQty(1);
    expect(qtyOf(1)).toBe(10);
  });

  it("removeActiveLine removes it and clears the selection", () => {
    store().setActiveLine(1);
    store().removeActiveLine();
    expect(store().cartOrder).toEqual([2, 3]);
    expect(store().activeLineItemId).toBeNull();
  });
});

describe("loadParkedCart", () => {
  it("drops items deleted since parking and clamps qty to current stock", () => {
    const catalogue = new Map([
      [1, item({ id: 1, stockQty: 2 })],
      [2, loose({ stockQty: 0 })],
    ]);
    store().loadParkedCart(
      [
        { itemId: 1, qty: 5 },
        { itemId: 2, qty: 3 },
        { itemId: 99, qty: 1 },
      ],
      1250,
      (id) => catalogue.get(id),
    );
    expect(store().cartOrder).toEqual([1, 2]);
    expect(qtyOf(1)).toBe(2);
    // Out of stock still keeps the line at its minimum, never at 0.
    expect(qtyOf(2)).toBe(0.01);
    expect(store().discountMode).toBe("flat");
    expect(store().discountValue).toBe(12.5);
  });
});

describe("clearCart", () => {
  it("resets the draft but keeps the payment method and advances the order number", () => {
    store().addItem(item());
    store().setPaymentMethod("card");
    const before = store().draftOrderNumber;
    store().clearCart();
    expect(store().cartOrder).toEqual([]);
    expect(store().paymentMethod).toBe("card");
    expect(store().draftOrderNumber).toBe(before + 1);
  });
});
