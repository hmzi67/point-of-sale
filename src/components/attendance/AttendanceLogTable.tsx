import { useEffect, useState } from "react";
import { getAttendance, getEmployees } from "../../services/attendanceService";
import { resolveDateRangePreset } from "../../utils/dateRanges";
import { formatHours, formatTime } from "../../utils/format";
import type { AttendanceRecord, Employee } from "../../types";
import { DateRangeInputs } from "../ui/DateRangeInputs";
import { useReport } from "../../hooks/useReport";

/** The attendance log: every check-in/out in a date range, filterable by
 * employee. An open shift (no check-out yet) shows "—" for both the out time
 * and the hours column rather than a wrong or fabricated number. */
export function AttendanceLogTable() {
  const [employees, setEmployees] = useState<Employee[]>([]);
  const [employeeId, setEmployeeId] = useState<number | "">("");
  const defaultRange = resolveDateRangePreset("thisWeek");
  const [startDate, setStartDate] = useState(defaultRange.startDate);
  const [endDate, setEndDate] = useState(defaultRange.endDate);

  const [employeesError, setEmployeesError] = useState<string | null>(null);

  useEffect(() => {
    getEmployees()
      .then(setEmployees)
      .catch((e: Error) => setEmployeesError(e.message));
  }, []);

  const log = useReport(
    () => getAttendance(employeeId === "" ? null : employeeId, startDate, endDate),
    [employeeId, startDate, endDate],
    { enabled: Boolean(startDate && endDate && startDate <= endDate) },
  );
  const records: AttendanceRecord[] = log.data ?? [];
  const isLoading = log.isLoading;
  const error = log.error ?? employeesError;

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <select
          value={employeeId}
          onChange={(e) => setEmployeeId(e.target.value === "" ? "" : Number(e.target.value))}
          className="rounded-xl border border-line-strong px-3 py-2.5 text-sm"
        >
          <option value="">All employees</option>
          {employees.map((employee) => (
            <option key={employee.id} value={employee.id}>
              {employee.name}
            </option>
          ))}
        </select>

        <DateRangeInputs label="Attendance" start={startDate} end={endDate} onStartChange={setStartDate} onEndChange={setEndDate} />
      </div>

      {error && <p className="rounded-xl bg-danger-soft px-3 py-2 text-sm text-danger-text">{error}</p>}

      <div className="overflow-x-auto rounded-2xl border border-line bg-surface">
        <table className="min-w-full divide-y divide-line text-sm">
          <thead className="bg-surface-2/60 text-left text-xs font-semibold text-fg-2">
            <tr>
              <th className="px-4 py-3.5 font-semibold">Date</th>
              <th className="px-4 py-3.5 font-semibold">Employee</th>
              <th className="px-4 py-3.5 font-semibold">Check in</th>
              <th className="px-4 py-3.5 font-semibold">Check out</th>
              <th className="px-4 py-3.5 text-right font-semibold">Hours</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-line">
            {isLoading ? (
              <tr>
                <td colSpan={5} className="px-4 py-12 text-center text-fg-muted">
                  Loading…
                </td>
              </tr>
            ) : records.length === 0 ? (
              <tr>
                <td colSpan={5} className="px-4 py-12 text-center text-fg-muted">
                  No attendance records in this range.
                </td>
              </tr>
            ) : (
              records.map((record) => {
                const incomplete = Boolean(record.checkIn) && !record.checkOut;
                return (
                  <tr key={record.id} className="transition-colors hover:bg-surface-2/50">
                    <td className="px-4 py-3.5 text-fg-2">{record.workDate}</td>
                    <td className="px-4 py-3.5 text-fg-2">{record.employeeName}</td>
                    <td className="px-4 py-3.5 text-fg-2">
                      {record.checkIn ? formatTime(record.checkIn) : "—"}
                    </td>
                    <td className="px-4 py-3.5 text-fg-2">
                      {record.checkOut ? (
                        formatTime(record.checkOut)
                      ) : incomplete ? (
                        <span className="rounded-lg bg-warning-soft px-2.5 py-1 text-xs font-medium text-warning-text">
                          in progress
                        </span>
                      ) : (
                        "—"
                      )}
                    </td>
                    <td className="px-4 py-3.5 whitespace-nowrap text-right text-fg-2 tabular-nums">
                      {record.hoursWorked !== null ? formatHours(record.hoursWorked) : "—"}
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
