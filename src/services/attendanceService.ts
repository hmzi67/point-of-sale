import type { AttendanceRecord, Employee, MonthlySummary } from "../types";
import { call } from "./tauriClient";

/** Active staff, for the check-in/out screen and the monthly summary. */
export function getEmployees(): Promise<Employee[]> {
  return call<Employee[]>("attendance_get_employees", {});
}

/** Checks an employee in for today. Idempotent — a repeated call the same
 * day updates the existing row rather than creating a second one. */
export function checkIn(employeeId: number): Promise<AttendanceRecord> {
  return call<AttendanceRecord>("attendance_check_in", { employeeId });
}

/** Checks an employee out for today. Rejects if they never checked in today. */
export function checkOut(employeeId: number): Promise<AttendanceRecord> {
  return call<AttendanceRecord>("attendance_check_out", { employeeId });
}

/** The attendance log for a date range (inclusive), optionally scoped to one
 * employee — omit `employeeId` for every employee's shifts in the range. */
export function getAttendance(
  employeeId: number | null,
  startDate: string,
  endDate: string,
): Promise<AttendanceRecord[]> {
  return call<AttendanceRecord[]>("attendance_get_attendance", { employeeId, startDate, endDate });
}

/** Days present/absent and total hours per employee for `month` (`YYYY-MM`). */
export function getMonthlySummary(month: string): Promise<MonthlySummary[]> {
  return call<MonthlySummary[]>("attendance_get_monthly_summary", { month });
}
