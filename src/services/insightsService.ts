import type {
  ExpiryRow,
  OrderProfitRow,
  PaymentTypeReport,
  PeriodGrouping,
  ProfitLoss,
  StockLevelReport,
  TaxReport,
  UsageRow,
  VoidReport,
} from "../types";
import { call } from "./tauriClient";

export function getProfitLoss(startDate: string, endDate: string, grouping: PeriodGrouping): Promise<ProfitLoss> {
  return call<ProfitLoss>("reports_get_profit_loss", { startDate, endDate, grouping });
}

export function getOrderProfit(startDate: string, endDate: string): Promise<OrderProfitRow[]> {
  return call<OrderProfitRow[]>("reports_get_order_profit", { startDate, endDate });
}

export function getPaymentTypes(startDate: string, endDate: string): Promise<PaymentTypeReport> {
  return call<PaymentTypeReport>("reports_get_payment_types", { startDate, endDate });
}

export function getTaxReport(startDate: string, endDate: string): Promise<TaxReport> {
  return call<TaxReport>("reports_get_tax", { startDate, endDate });
}

export function getVoidReport(startDate: string, endDate: string): Promise<VoidReport> {
  return call<VoidReport>("reports_get_voids", { startDate, endDate });
}

export function getStockLevels(): Promise<StockLevelReport> {
  return call<StockLevelReport>("reports_get_stock_levels", {});
}

export function getInventoryUsage(startDate: string, endDate: string): Promise<UsageRow[]> {
  return call<UsageRow[]>("reports_get_inventory_usage", { startDate, endDate });
}

export function getExpiryReport(withinDays: number): Promise<ExpiryRow[]> {
  return call<ExpiryRow[]>("reports_get_expiry", { withinDays });
}
