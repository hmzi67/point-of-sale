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

  /** `null` when the `tables` module is disabled, or when it's enabled but
   * no table-attributed sale happened in the range. */
  topTableBySales: TopTable | null;

  /** `null` when the `delivery` module is disabled for this installation. */
  deliveryStats: DeliveryDashboardStats | null;
}

export interface DeliveryDashboardStats {
  orderCount: number;
  salesMinor: number;
}

export interface TopTable {
  tableId: number;
  name: string;
  totalMinor: number;
}
