import type { DailySales, SalesSummary, TopItem, TopItemSort } from "../types";
import { call } from "./tauriClient";

/** `startDate`/`endDate` are `YYYY-MM-DD`, inclusive. */
export function getSalesSummary(startDate: string, endDate: string): Promise<SalesSummary> {
  return call<SalesSummary>("reports_get_sales_summary", { startDate, endDate });
}

export function getTopItems(
  startDate: string,
  endDate: string,
  limit: number,
  sortBy: TopItemSort,
): Promise<TopItem[]> {
  return call<TopItem[]>("reports_get_top_items", { startDate, endDate, limit, sortBy });
}

/** One row per calendar day in the range, zero-filled where there were no sales. */
export function getSalesOverTime(startDate: string, endDate: string): Promise<DailySales[]> {
  return call<DailySales[]>("reports_get_sales_over_time", { startDate, endDate });
}
