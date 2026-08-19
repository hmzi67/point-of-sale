/** Employee attendance — see `src-tauri/src/db/attendance.rs`. */

export interface Employee {
  id: number;
  name: string;
  role: string;
  contact: string | null;
  baseSalaryMinor: number;
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
