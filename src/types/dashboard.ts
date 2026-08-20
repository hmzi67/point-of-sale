/** The owner/admin dashboard aggregate — see `src-tauri/src/db/dashboard.rs`. */

export interface DashboardSummary {
  startDate: string;
  endDate: string;

  totalSalesMinor: number;
  /** Refunds recorded in the range — always counted, unlike the
   * module-gated fields below (refunds aren't a toggleable module). */
  refundsMinor: number;
  transactionCount: number;

  /** `null` when the `expenses` module is disabled for this installation —
   * not folded into `netProfitMinor` either. */
  totalExpensesMinor: number | null;
  /** `null` when the `salary` module is disabled — same treatment. */
  totalSalaryPaidMinor: number | null;
  netProfitMinor: number;

  /** `null` when the `inventory` module is disabled. */
  lowStockItemCount: number | null;
}
