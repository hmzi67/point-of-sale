import { describe, expect, it } from "vitest";
import { CODE128_PATTERNS, code128Modules, code128Values } from "./code128";

describe("Code 128 patterns", () => {
  it("has 107 symbols, each 11 modules wide (stop: 13) with an even bar count", () => {
    expect(CODE128_PATTERNS).toHaveLength(107);
    CODE128_PATTERNS.forEach((pattern, value) => {
      const widths = [...pattern].map(Number);
      expect(widths.reduce((a, b) => a + b, 0)).toBe(value === 106 ? 13 : 11);
      const bars = widths.filter((_, i) => i % 2 === 0).reduce((a, b) => a + b, 0);
      expect(bars % 2).toBe(0);
    });
    expect(new Set(CODE128_PATTERNS).size).toBe(107);
  });
});

describe("code128Values", () => {
  it("uses set B for text and computes the mod-103 checksum", () => {
    // Start B (104) + P(48)·1 + J(42)·2 + J·3 + 1(17)·4 + 2(18)·5 + 3(19)·6 + C(35)·7 = 879 → 879 % 103 = 55.
    expect(code128Values("PJJ123C")).toEqual([104, 48, 42, 42, 17, 18, 19, 35, 55, 106]);
  });

  it("packs an even run of digits two per symbol with set C", () => {
    expect(code128Values("200000000017")).toEqual([105, 20, 0, 0, 0, 0, 17, (105 + 20 + 17 * 6) % 103, 106]);
  });

  it("refuses characters a barcode can't carry", () => {
    expect(() => code128Values("café")).toThrow();
  });

  it("starts and ends on a bar", () => {
    const modules = code128Modules("ABC");
    expect(modules.length % 2).toBe(1);
  });
});
