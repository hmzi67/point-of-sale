import type { Shift, ShiftSummary } from "../types";
import { call } from "./tauriClient";

/** The signed-in cashier's currently-open shift, if any. */
export function getOpenShift(): Promise<Shift | null> {
  return call<Shift | null>("shift_get_open", {});
}

/** Opens a new shift for the signed-in cashier; rejects if they already have one open. */
export function openShift(openingBalanceMinor: number): Promise<Shift> {
  return call<Shift>("shift_open", { openingBalanceMinor });
}

/** Reconciliation breakdown for `shiftId`. Pass `declaredCashAmountMinor` to
 * preview "if I declared this much, what would Short/Over be" before
 * confirming a close; omit (`null`) to read what's actually stored. */
export function getShiftSummary(shiftId: number, declaredCashAmountMinor: number | null): Promise<ShiftSummary> {
  return call<ShiftSummary>("shift_get_summary", { shiftId, declaredCashAmountMinor });
}

/** Closes `shiftId` with the cashier's declared cash count. */
export function closeShift(shiftId: number, declaredCashAmountMinor: number): Promise<ShiftSummary> {
  return call<ShiftSummary>("shift_close", { shiftId, declaredCashAmountMinor });
}

/** Shift history for the Shifts page, newest first. */
export function listRecentShifts(limit: number): Promise<Shift[]> {
  return call<Shift[]>("shift_list_recent", { limit });
}

/** Prints a shift's close-out reconciliation receipt. */
export function printShiftSummaryThermal(shiftId: number): Promise<void> {
  return call<void>("shift_print_summary", { shiftId });
}
