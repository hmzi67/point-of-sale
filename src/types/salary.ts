/** Salary calculation and payments — see `src-tauri/src/db/salary.rs`. */

export type PaymentStatus = "unpaid" | "partial" | "paid";

export interface SalaryCalculation {
  employeeId: number;
  employeeName: string;
  /** `YYYY-MM`. */
  month: string;
  baseSalaryMinor: number;
  workingDaysInMonth: number;
  daysPresent: number;
  calculatedAmountMinor: number;
  paidAmountMinor: number;
  paidDate: string | null;
  status: PaymentStatus;
}
