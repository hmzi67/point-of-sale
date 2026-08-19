import type { DateRange, DateRangePreset } from "../types";

/** `YYYY-MM-DD` in local time — never `toISOString()`, which is UTC and can
 * land on the wrong day for the cashier's actual calendar date. */
function toDateString(date: Date): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function startOfWeek(date: Date): Date {
  const result = new Date(date);
  // Monday-first week, matching a typical retail trading week.
  const day = (result.getDay() + 6) % 7;
  result.setDate(result.getDate() - day);
  return result;
}

function startOfMonth(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth(), 1);
}

/** Resolves a preset to a concrete `{ startDate, endDate }`, both inclusive
 * and both anchored to today in local time. */
export function resolveDateRangePreset(preset: Exclude<DateRangePreset, "custom">, today = new Date()): DateRange {
  const todayStr = toDateString(today);
  switch (preset) {
    case "today":
      return { startDate: todayStr, endDate: todayStr };
    case "thisWeek":
      return { startDate: toDateString(startOfWeek(today)), endDate: todayStr };
    case "thisMonth":
      return { startDate: toDateString(startOfMonth(today)), endDate: todayStr };
  }
}

export const PRESET_LABELS: Record<Exclude<DateRangePreset, "custom">, string> = {
  today: "Today",
  thisWeek: "This Week",
  thisMonth: "This Month",
};
