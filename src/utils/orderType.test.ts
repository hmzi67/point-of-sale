import { describe, expect, it } from "vitest";
import { isDeliveryOrder } from "./orderType";

describe("isDeliveryOrder", () => {
  it("is true only for the two delivery types", () => {
    expect(isDeliveryOrder("delivery")).toBe(true);
    expect(isDeliveryOrder("third_party_delivery")).toBe(true);
    expect(isDeliveryOrder("takeaway")).toBe(false);
    expect(isDeliveryOrder("dine_in")).toBe(false);
    expect(isDeliveryOrder(null)).toBe(false);
  });
});
