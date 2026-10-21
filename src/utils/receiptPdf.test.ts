import { describe, expect, it } from "vitest";
import { receiptOrderField, riderText } from "./receiptPdf";

// Same cases as `printer::escpos::tests::order_field_matches_the_pdf_receipt`.
const base = { tableName: null, orderType: null, deliveryPlatformName: null, orderNo: null };

describe("receiptOrderField", () => {
  it.each([
    [{ ...base, tableName: "T4", orderType: "dine_in" }, true, "Table: T4"],
    [{ ...base, orderType: "delivery" }, true, "Type: Delivery"],
    [{ ...base, orderType: "third_party_delivery", deliveryPlatformName: "Foodpanda" }, true, "Type: Foodpanda"],
    [{ ...base, orderType: "third_party_delivery" }, true, "Type: 3rd-Party"],
    [{ ...base, orderType: "takeaway" }, false, "Type: Takeaway"],
    [{ ...base, orderType: "dine_in" }, false, "Type: Dine-In"],
    [{ ...base }, true, "Type: Takeaway"],
    [{ ...base }, false, "Type: Counter Sale"],
    [{ ...base, orderType: "takeaway", orderNo: 17 }, false, "Type: Takeaway (#17)"],
  ])("%o (tables %s) → %s", (sale, tablesEnabled, expected) => {
    expect(receiptOrderField(sale, tablesEnabled)).toBe(expected);
  });
});

describe("riderText", () => {
  it("adds the phone only when there is one", () => {
    expect(riderText("Ali", "0300")).toBe("Ali (0300)");
    expect(riderText("Ali", null)).toBe("Ali");
    expect(riderText("Ali", "")).toBe("Ali");
  });
});
