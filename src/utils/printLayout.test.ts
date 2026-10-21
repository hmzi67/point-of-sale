import { describe, expect, it } from "vitest";
import type { jsPDF } from "jspdf";
import { clipToWidth } from "./printLayout";

// One unit of width per character: enough to test the truncation logic
// without rendering fonts.
const doc = { getTextWidth: (text: string) => text.length } as unknown as jsPDF;

describe("clipToWidth", () => {
  it("leaves text that fits untouched", () => {
    expect(clipToWidth(doc, "Tea", 3)).toBe("Tea");
  });

  it("trims text that overflows to the widest prefix that fits", () => {
    expect(clipToWidth(doc, "Chicken Karahi", 7)).toBe("Chicken");
  });

  it("never returns an empty string", () => {
    expect(clipToWidth(doc, "Biryani", 0)).toBe("B");
  });
});
