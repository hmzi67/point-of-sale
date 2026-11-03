import { describe, expect, it } from "vitest";
import { monthGrid } from "./monthCalendar";

describe("monthGrid", () => {
  it("starts October 2026 on its Thursday, with 31 days", () => {
    const { blanks, dates } = monthGrid(2026, 9);
    expect(blanks).toBe(3);
    expect(dates).toHaveLength(31);
    expect([dates[0], dates[30]]).toEqual(["2026-10-01", "2026-10-31"]);
  });

  it("handles a Sunday first and a leap February", () => {
    expect(monthGrid(2026, 1).blanks).toBe(6);
    expect(monthGrid(2028, 1).dates).toHaveLength(29);
  });
});
