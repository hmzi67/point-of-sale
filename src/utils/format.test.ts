import { describe, expect, it } from "vitest";
import {
  decimalToMinor,
  formatCompactTick,
  formatHours,
  formatMinor,
  formatQty,
  formatTime,
  minorToDecimal,
  parseLocalDateString,
} from "./format";

describe("money conversion", () => {
  it("converts a decimal amount to whole minor units, absorbing float error", () => {
    expect(decimalToMinor(0.1 + 0.2)).toBe(30);
    expect(decimalToMinor(19.99)).toBe(1999);
    expect(decimalToMinor(1.005)).toBe(100); // 1.005 is 1.00499… in binary
  });

  it("round-trips minor units through a decimal", () => {
    for (const minor of [0, 1, 99, 100, 123456]) {
      expect(decimalToMinor(minorToDecimal(minor))).toBe(minor);
    }
  });

  it("formats minor units with the currency and two decimals", () => {
    expect(formatMinor(123456, "PKR")).toBe("PKR 1234.56");
    expect(formatMinor(5, "USD")).toBe("USD 0.05");
  });
});

describe("formatQty", () => {
  it("drops trailing zeros and appends the unit", () => {
    expect(formatQty(2)).toBe("2");
    expect(formatQty(0.77, "kg")).toBe("0.77 kg");
    expect(formatQty(1.5)).toBe("1.5");
    expect(formatQty(0.3333333)).toBe("0.33");
  });
});

describe("formatCompactTick", () => {
  it("abbreviates thousands only", () => {
    expect(formatCompactTick(999)).toBe("999");
    expect(formatCompactTick(1000)).toBe("1k");
    expect(formatCompactTick(12_600)).toBe("13k");
  });
});

describe("parseLocalDateString", () => {
  it("parses as local midnight, not UTC", () => {
    const d = parseLocalDateString("2026-10-01");
    expect([d.getFullYear(), d.getMonth(), d.getDate(), d.getHours()]).toEqual([2026, 9, 1, 0]);
  });
});

describe("formatTime", () => {
  it("falls back to the raw string when it isn't a SQLite timestamp", () => {
    expect(formatTime("not a timestamp")).toBe("not a timestamp");
    expect(formatTime("2026-10-01")).toBe("2026-10-01");
  });

  it("formats the local time of a well-formed timestamp", () => {
    const expected = new Date(2026, 9, 1, 9, 5, 0).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
    expect(formatTime("2026-10-01 09:05:00")).toBe(expected);
  });
});

describe("formatHours", () => {
  it("drops the minutes for whole hours", () => {
    expect(formatHours(8)).toBe("8h");
    expect(formatHours(0)).toBe("0h");
  });

  it("shows hours and minutes", () => {
    expect(formatHours(8.5)).toBe("8h 30m");
    expect(formatHours(0.25)).toBe("0h 15m");
  });

  it("rolls 60 rounded minutes over into the next hour", () => {
    expect(formatHours(1.999)).toBe("2h");
  });
});
