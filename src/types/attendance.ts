/** Employee attendance — see `src-tauri/src/db/attendance.rs`. */

export interface Employee {
  id: number;
  name: string;
  role: string;
  contact: string | null;
  baseSalaryMinor: number;
}

/** An employee as shown on the employee management screen — includes
 * whether the record is active, unlike `Employee` (the check-in screen and
 * the monthly summary only ever see active ones). */
export interface ManagedEmployee extends Employee {
  isActive: boolean;
}

export interface AttendanceRecord {
  id: number;
  employeeId: number;
  employeeName: string;
  /** `YYYY-MM-DD`. */
  workDate: string;
  checkIn: string | null;
  checkOut: string | null;
  /** `null` whenever `checkOut` is `null` — nothing to measure yet. */
  hoursWorked: number | null;
}

export interface MonthlySummary {
  employeeId: number;
  employeeName: string;
  daysPresent: number;
  daysAbsent: number;
  /** Shifts with a check-in but no check-out — already counted in
   * `daysPresent`, called out separately so a forgotten clock-out doesn't
   * silently read as a zero-hour day. */
  incompleteDays: number;
  totalHours: number;
}
