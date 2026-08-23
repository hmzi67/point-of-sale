import type { AttendanceRecord, Employee, ManagedEmployee, MonthlySummary } from "../types";
import { call } from "./tauriClient";

/** Active staff, for the check-in/out screen and the monthly summary. */
export function getEmployees(): Promise<Employee[]> {
  return call<Employee[]>("attendance_get_employees", {});
}

/** Every employee, active or not — the employee management screen. */
export function getAllEmployees(): Promise<ManagedEmployee[]> {
  return call<ManagedEmployee[]>("attendance_get_all_employees", {});
}

/** Adds a new employee (payroll/attendance record, not a login account).
 * Shows up on the check-in screen and the monthly summary immediately. */
export function addEmployee(
  name: string,
  role: string,
  contact: string | null,
  baseSalaryMinor: number,
): Promise<ManagedEmployee> {
  return call<ManagedEmployee>("attendance_add_employee", { name, role, contact, baseSalaryMinor });
}

export function updateEmployee(
  employeeId: number,
  name: string,
  role: string,
  contact: string | null,
  baseSalaryMinor: number,
): Promise<ManagedEmployee> {
  return call<ManagedEmployee>("attendance_update_employee", { employeeId, name, role, contact, baseSalaryMinor });
}

/** Deactivates or reactivates an employee (soft — attendance/salary history
 * stays intact). A deactivated employee drops off the check-in screen and
 * the monthly summary immediately. */
export function setEmployeeActive(employeeId: number, isActive: boolean): Promise<void> {
  return call<void>("attendance_set_employee_active", { employeeId, isActive });
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
