import { describe, expect, it } from "vitest";
import { orderTypeLabel, receiptAmount, receiptDate, riderText } from "./receiptFormat";

// Same cases as `printer::escpos::tests::the_order_type_and_optional_rows_follow_the_sale`.
const base = { tableName: null, orderType: null, deliveryPlatformName: null };

describe("orderTypeLabel", () => {
  it.each([
    [{ ...base, tableName: "T4", orderType: "dine_in" }, true, "Dine In"],
    [{ ...base, tableName: "T4" }, true, "Dine In"],
    [{ ...base, orderType: "delivery" }, true, "Delivery"],
    [{ ...base, orderType: "third_party_delivery", deliveryPlatformName: "Foodpanda" }, true, "Foodpanda"],
    [{ ...base, orderType: "third_party_delivery" }, true, "3rd-Party"],
    [{ ...base }, true, "Takeaway"],
    [{ ...base }, false, "Counter Sale"],
  ])("%o (tables %s) → %s", (sale, tablesEnabled, expected) => {
    expect(orderTypeLabel(sale, tablesEnabled)).toBe(expected);
  });
});

describe("receipt formatting", () => {
  it("groups thousands and keeps paisa only when there are some", () => {
    expect(receiptAmount(125_000)).toBe("1,250");
    expect(receiptAmount(13_000)).toBe("130");
    expect(receiptAmount(123_456_789)).toBe("1,234,567.89");
    expect(receiptAmount(-5_050)).toBe("-50.50");
  });

  it("prints the date day-first with a 12-hour clock", () => {
    expect(receiptDate("2026-10-17 02:25:00")).toBe("17/10/2026 02:25 AM");
    expect(receiptDate("2026-10-17 14:05:00")).toBe("17/10/2026 02:05 PM");
    expect(receiptDate("2026-10-17 00:10:00")).toBe("17/10/2026 12:10 AM");
  });

  it("adds the rider's phone only when there is one", () => {
    expect(riderText("Ali", "0300")).toBe("Ali (0300)");
    expect(riderText("Ali", null)).toBe("Ali");
  });
});
