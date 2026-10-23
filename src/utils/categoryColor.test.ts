import { describe, expect, it } from "vitest";
import { categoryColor } from "./categoryColor";

describe("categoryColor", () => {
  it("is stable per category and neutral without one", () => {
    expect(categoryColor(7)).toBe(categoryColor(7));
    expect(categoryColor(null)).toEqual(categoryColor(undefined));
    expect(categoryColor(null).bg).toBe("bg-surface-3");
  });

  it("cycles through the palette", () => {
    const seen = new Set(Array.from({ length: 50 }, (_, id) => categoryColor(id).bg));
    expect(seen.size).toBeGreaterThan(1);
    expect(categoryColor(0)).toBe(categoryColor(seen.size));
  });
});
