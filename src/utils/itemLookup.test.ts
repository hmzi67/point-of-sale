import { describe, expect, it } from "vitest";
import type { Item } from "../types";
import { findItemByCode } from "./itemLookup";

const item = (id: number, barcode: string | null, shortCode: string | null) =>
  ({ id, barcode, shortCode }) as Item;

describe("findItemByCode", () => {
  const items = [item(1, "1234", null), item(2, null, "12"), item(3, "999", "1234")];

  it("prefers an exact barcode match over another item's short code", () => {
    expect(findItemByCode(items, "1234")?.id).toBe(1);
  });

  it("falls back to the short code", () => {
    expect(findItemByCode(items, "12")?.id).toBe(2);
  });

  it("finds nothing for an unknown or prefix-only code", () => {
    expect(findItemByCode(items, "123")).toBeUndefined();
    expect(findItemByCode([], "1234")).toBeUndefined();
  });
});
