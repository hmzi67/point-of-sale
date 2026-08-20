/** Money fields cross IPC as integer minor units, matching the Rust side. */

export interface Shift {
  id: number;
  cashierId: number | null;
  cashierName: string | null;
  openedAt: string;
  /** `null` while the shift is still open. */
  closedAt: string | null;
  openingBalanceMinor: number;
  /** `null` until the shift is closed. */
  declaredCashAmountMinor: number | null;
  notes: string | null;
}

export interface ShiftSummary {
  shift: Shift;
  openingBalanceMinor: number;
  cashSalesMinor: number;
  cardSalesMinor: number;
  otherSalesMinor: number;
  /** Always 0 — this product has no credit/tab sale concept. */
  creditSalesMinor: number;
  totalSalesMinor: number;
  discountMinor: number;
  refundsMinor: number;
  /** `openingBalanceMinor + cashSalesMinor - refundsMinor`. */
  expectedCashMinor: number;
  /** `null` until a declared amount exists (either persisted, or a live preview). */
  declaredCashAmountMinor: number | null;
  /** `declared - expected`: negative is Short, positive is Over. `null` alongside `declaredCashAmountMinor`. */
  differenceMinor: number | null;
}
