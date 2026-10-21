/** Money fields cross IPC as integer minor units, matching the Rust side. */

import type { DashboardSummary } from "./dashboard";
import type { Refund } from "./refunds";

export interface SalesSummary {
  startDate: string;
  endDate: string;
  /** Gross — unaffected by any refund. */
  totalSalesMinor: number;
  /** Refunds recorded in the range (by refund date, not original sale date). */
  refundsMinor: number;
  /** `totalSalesMinor - refundsMinor` — what profit math should use. */
  netSalesMinor: number;
  transactionCount: number;
  averageSaleMinor: number;
}

export type TopItemSort = "quantity" | "revenue";

export interface TopItem {
  itemId: number;
  itemName: string;
  qtySold: number;
  revenueMinor: number;
}

export interface DailySales {
  /** `YYYY-MM-DD`. */
  date: string;
  totalMinor: number;
  refundsMinor: number;
  netMinor: number;
  transactionCount: number;
}

export interface CategorySalesLine {
  itemId: number;
  itemName: string;
  qtySold: number;
  revenueMinor: number;
}

export interface CategorySalesGroup {
  categoryId: number | null;
  categoryName: string;
  items: CategorySalesLine[];
  subtotalMinor: number;
}

export interface CategorySalesReport {
  startDate: string;
  endDate: string;
  groups: CategorySalesGroup[];
  grandTotalMinor: number;
}

export interface TableSalesRow {
  /** `null` for the "Counter / Takeaway" row. */
  tableId: number | null;
  label: string;
  totalMinor: number;
  transactionCount: number;
}

export interface TableSalesSummary {
  startDate: string;
  endDate: string;
  /** Highest-total first. */
  rows: TableSalesRow[];
  grandTotalMinor: number;
}

/** The "Refunds" report — every refund in the range (by refund date, not
 * the date of the original sale), most recent first. Reuses the same
 * `Refund` shape `refunds_get`/the reprint flow already return, so this row
 * and a single reprinted refund can never drift in shape. */
export interface RefundsSummary {
  startDate: string;
  endDate: string;
  /** Most recent first. */
  refunds: Refund[];
  grandTotalRefundedMinor: number;
}

export interface ProductSalesRow {
  itemId: number;
  itemName: string;
  categoryId: number | null;
  categoryName: string;
  qtySold: number;
  revenueMinor: number;
  /** 1-based position under the report's `sortBy`. */
  rank: number;
}

export interface ProductSalesNoSaleRow {
  itemId: number;
  itemName: string;
  categoryId: number | null;
  categoryName: string;
}

export interface ProductSalesSummaryReport {
  startDate: string;
  endDate: string;
  sortBy: TopItemSort;
  /** Every item with at least one sale in the range, ranked. */
  rows: ProductSalesRow[];
  /** Active items with zero sales in the range — shown as their own
   * section so slow-moving stock stays visible, not hidden. */
  noSalesItems: ProductSalesNoSaleRow[];
}

export interface PaymentMethodBreakdown {
  cashMinor: number;
  cashCount: number;
  cardMinor: number;
  cardCount: number;
  otherMinor: number;
  otherCount: number;
}

export interface SalesMetrics {
  totalDiscountMinor: number;
  totalTaxMinor: number;
  netSalesMinor: number;
  totalItemsSold: number;
}

/** The "Overall Sales Report" consolidated document — see
 * `src-tauri/src/db/full_report.rs`. `tableSales` is `null` when the
 * `tables` module is disabled for this installation/platform, same
 * "not tracked, not a zero" convention `DashboardSummary` uses. */
export interface FullReport {
  startDate: string;
  endDate: string;
  overview: DashboardSummary;
  averageSaleMinor: number;
  paymentMethods: PaymentMethodBreakdown;
  salesMetrics: SalesMetrics;
  /** Itemized refunds for the period, plus a grand total — never
   * module-gated, so always present, unlike `tableSales` below. */
  refunds: RefundsSummary;
  categorySales: CategorySalesReport;
  productSales: ProductSalesSummaryReport;
  tableSales: TableSalesSummary | null;
}

export type DateRangePreset = "today" | "thisWeek" | "thisMonth" | "custom";

export interface DateRange {
  startDate: string;
  endDate: string;
}

export interface PlatformSummary {
  platformId: number | null;
  platformName: string;
  orderCount: number;
  totalSalesMinor: number;
}

export interface RiderSummary {
  riderKey: string;
  riderName: string;
  riderPhone: string | null;
  riderSource: string;
  orderCount: number;
  deliveredCount: number;
  totalSalesMinor: number;
  deliveryChargesMinor: number;
}

export interface DeliveryOrderRow {
  id: number;
  orderNo: number;
  orderType: string;
  deliveryStatus: string | null;
  platformName: string | null;
  riderName: string | null;
  riderPhone: string | null;
  customerName: string | null;
  customerPhone: string | null;
  deliveryAddress: string | null;
  deliveryChargeMinor: number;
  totalMinor: number;
  createdAt: string;
}

export interface DeliveryReport {
  startDate: string;
  endDate: string;
  totalOrders: number;
  inHouseOrders: number;
  thirdPartyOrders: number;
  totalSalesMinor: number;
  inHouseSalesMinor: number;
  thirdPartySalesMinor: number;
  totalDeliveryChargesMinor: number;
  platforms: PlatformSummary[];
  riders: RiderSummary[];
  orders: DeliveryOrderRow[];
}
