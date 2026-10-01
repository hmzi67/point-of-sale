import { describe, expect, it } from "vitest";
import { formatTimeUntil } from "./licenseFormat";

const NOW = 1_800_000_000;

describe("formatTimeUntil", () => {
  it("says now for the past, the present and under a minute", () => {
    expect(formatTimeUntil(NOW - 3600, NOW)).toBe("now");
    expect(formatTimeUntil(NOW, NOW)).toBe("now");
    expect(formatTimeUntil(NOW + 59, NOW)).toBe("now");
  });

  it("uses singular and plural units", () => {
    expect(formatTimeUntil(NOW + 60, NOW)).toBe("in 1 minute");
    expect(formatTimeUntil(NOW + 45 * 60, NOW)).toBe("in 45 minutes");
    expect(formatTimeUntil(NOW + 3600, NOW)).toBe("in 1 hour");
    expect(formatTimeUntil(NOW + 5 * 86400, NOW)).toBe("in 5 days");
  });

  it("never prints 60 minutes — rounding up to an hour switches unit", () => {
    expect(formatTimeUntil(NOW + 3570, NOW)).toBe("in 1 hour");
  });

  it("switches from hours to days at 48 hours", () => {
    expect(formatTimeUntil(NOW + 47 * 3600, NOW)).toBe("in 47 hours");
    expect(formatTimeUntil(NOW + 48 * 3600, NOW)).toBe("in 2 days");
  });
});
