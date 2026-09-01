import type { SalaryCalculation } from "../types";
import { call } from "./tauriClient";

/** `base_salary / working_days_per_month * days_present`, refreshed against
 * the latest attendance every call. */
export function calculateSalary(employeeId: number, month: string): Promise<SalaryCalculation> {
  return call<SalaryCalculation>("salary_calculate_salary", { employeeId, month });
}

/** Every active employee's calculated salary, amount paid so far, and status
 * for `month` — the monthly overview table. */
export function getMonthlyOverview(month: string): Promise<SalaryCalculation[]> {
  return call<SalaryCalculation[]>("salary_get_monthly_overview", { month });
}

/** Prints the Employee Report (attendance + salary for every active
 * employee in `month`, same rows `getMonthlyOverview` returns) on a USB
 * thermal printer. */
export function printMonthlyReportThermal(month: string): Promise<void> {
  return call<void>("salary_print_monthly_report", { month });
}

/** Records a payment against `month`'s salary — added to whatever has
 * already been paid that month, not a replacement. */
export function recordPayment(
  employeeId: number,
  month: string,
  paidAmountMinor: number,
  paidDate: string,
): Promise<SalaryCalculation> {
  return call<SalaryCalculation>("salary_record_payment", { employeeId, month, paidAmountMinor, paidDate });
}

/** Every month with a salary record for one employee, most recent first. */
export function getPaymentHistory(employeeId: number): Promise<SalaryCalculation[]> {
  return call<SalaryCalculation[]>("salary_get_payment_history", { employeeId });
}
