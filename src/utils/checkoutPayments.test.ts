import { describe, expect, it } from "vitest";
import { buildCheckoutPayments, defaultDueDate } from "./checkoutPayments";

describe("buildCheckoutPayments", () => {
  it("leaves a single-method sale as just its method", () => {
    expect(buildCheckoutPayments("card", 5000, [], "PKR")).toEqual({ paymentMethod: "card", creditMinor: 0, error: null });
  });

  it("puts the whole payable amount on credit for pay later", () => {
    const result = buildCheckoutPayments("credit", 5000, [], "PKR");
    expect(result.payments).toEqual([{ method: "credit", amountMinor: 5000 }]);
    expect(result.creditMinor).toBe(5000);
  });

  it("accepts split rows that add up exactly, skipping blanks", () => {
    const result = buildCheckoutPayments(
      "split",
      5000,
      [
        { method: "cash", amount: "20" },
        { method: "card", amount: "" },
        { method: "credit", amount: "30" },
      ],
      "PKR",
    );
    expect(result.error).toBeNull();
    expect(result.payments).toEqual([
      { method: "cash", amountMinor: 2000 },
      { method: "credit", amountMinor: 3000 },
    ]);
    expect(result.creditMinor).toBe(3000);
  });

  it("explains a split that is short or over", () => {
    expect(buildCheckoutPayments("split", 5000, [{ method: "cash", amount: "20" }], "PKR").error).toMatch(/still to allocate/);
    expect(buildCheckoutPayments("split", 5000, [{ method: "cash", amount: "60" }], "PKR").error).toMatch(/over the total/);
    expect(buildCheckoutPayments("split", 5000, [], "PKR").error).toMatch(/how the bill/);
  });
});

describe("defaultDueDate", () => {
  it("counts calendar days forward across a month end", () => {
    expect(defaultDueDate(7, new Date(2026, 0, 28))).toBe("2026-02-04");
  });
});
