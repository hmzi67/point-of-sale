import { afterEach, describe, expect, it, vi } from "vitest";
import { currentMonth, resolveDateRangePreset, toDateString, todayString } from "./dateRanges";

afterEach(() => vi.useRealTimers());

describe("local date helpers", () => {
  it("formats in local time, zero-padded", () => {
    expect(toDateString(new Date(2026, 0, 5))).toBe("2026-01-05");
  });

  it("derives today and the current month from the same local clock", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date(2026, 9, 1, 0, 30));
    expect(todayString()).toBe("2026-10-01");
    expect(currentMonth()).toBe("2026-10");
  });
});

describe("resolveDateRangePreset", () => {
  it("today is a single-day range", () => {
    expect(resolveDateRangePreset("today", new Date(2026, 9, 1))).toEqual({ startDate: "2026-10-01", endDate: "2026-10-01" });
  });

  it("this week starts on Monday", () => {
    // Thursday 2026-10-01 → Monday 2026-09-28, across a month boundary.
    expect(resolveDateRangePreset("thisWeek", new Date(2026, 9, 1))).toEqual({
      startDate: "2026-09-28",
      endDate: "2026-10-01",
    });
  });

  it("a Sunday belongs to the week that began six days earlier", () => {
    expect(resolveDateRangePreset("thisWeek", new Date(2026, 9, 4))).toEqual({
      startDate: "2026-09-28",
      endDate: "2026-10-04",
    });
  });

  it("a Monday is its own week start", () => {
    expect(resolveDateRangePreset("thisWeek", new Date(2026, 8, 28)).startDate).toBe("2026-09-28");
  });

  it("this week can cross a year boundary", () => {
    // Friday 2027-01-01 → Monday 2026-12-28.
    expect(resolveDateRangePreset("thisWeek", new Date(2027, 0, 1)).startDate).toBe("2026-12-28");
  });

  it("this month starts on the 1st", () => {
    expect(resolveDateRangePreset("thisMonth", new Date(2026, 9, 17))).toEqual({
      startDate: "2026-10-01",
      endDate: "2026-10-17",
    });
  });
});
