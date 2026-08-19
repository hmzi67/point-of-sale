import { useEffect, useState } from "react";
import { getAttendance, getEmployees } from "../../services/attendanceService";
import { resolveDateRangePreset } from "../../utils/dateRanges";
import { formatHours, formatTime } from "../../utils/format";
import type { AttendanceRecord, Employee } from "../../types";

/** The attendance log: every check-in/out in a date range, filterable by
 * employee. An open shift (no check-out yet) shows "—" for both the out time
 * and the hours column rather than a wrong or fabricated number. */
export function AttendanceLogTable() {
  const [employees, setEmployees] = useState<Employee[]>([]);
  const [employeeId, setEmployeeId] = useState<number | "">("");
  const defaultRange = resolveDateRangePreset("thisWeek");
  const [startDate, setStartDate] = useState(defaultRange.startDate);
  const [endDate, setEndDate] = useState(defaultRange.endDate);

  const [records, setRecords] = useState<AttendanceRecord[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getEmployees()
      .then(setEmployees)
      .catch((e: Error) => setError(e.message));
  }, []);

  useEffect(() => {
    if (!startDate || !endDate || startDate > endDate) return;
    setIsLoading(true);
    setError(null);
    getAttendance(employeeId === "" ? null : employeeId, startDate, endDate)
      .then(setRecords)
      .catch((e: Error) => setError(e.message))
      .finally(() => setIsLoading(false));
  }, [employeeId, startDate, endDate]);

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <select
          value={employeeId}
          onChange={(e) => setEmployeeId(e.target.value === "" ? "" : Number(e.target.value))}
          className="rounded-md border border-slate-300 px-2.5 py-1.5 text-sm"
        >
          <option value="">All employees</option>
          {employees.map((employee) => (
            <option key={employee.id} value={employee.id}>
              {employee.name}
            </option>
          ))}
        </select>

        <div className="flex items-center gap-1.5">
          <input
            type="date"
            value={startDate}
            max={endDate || undefined}
            onChange={(e) => setStartDate(e.target.value)}
            className="rounded-md border border-slate-300 px-2.5 py-1.5 text-sm"
          />
          <span className="text-slate-400">–</span>
          <input
            type="date"
            value={endDate}
            min={startDate || undefined}
            onChange={(e) => setEndDate(e.target.value)}
            className="rounded-md border border-slate-300 px-2.5 py-1.5 text-sm"
          />
        </div>
      </div>

      {error && <p className="rounded-md bg-red-50 px-3 py-2 text-sm text-red-700">{error}</p>}

      <div className="overflow-x-auto rounded-lg border border-slate-200 bg-white">
        <table className="min-w-full divide-y divide-slate-200 text-sm">
          <thead className="bg-slate-50 text-left text-xs font-medium uppercase tracking-wide text-slate-500">
            <tr>
              <th className="px-4 py-2">Date</th>
              <th className="px-4 py-2">Employee</th>
              <th className="px-4 py-2">Check in</th>
              <th className="px-4 py-2">Check out</th>
              <th className="px-4 py-2 text-right">Hours</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-100">
            {isLoading ? (
              <tr>
                <td colSpan={5} className="px-4 py-6 text-center text-slate-400">
                  Loading…
                </td>
              </tr>
            ) : records.length === 0 ? (
              <tr>
                <td colSpan={5} className="px-4 py-6 text-center text-slate-400">
                  No attendance records in this range.
                </td>
              </tr>
            ) : (
              records.map((record) => {
                const incomplete = Boolean(record.checkIn) && !record.checkOut;
                return (
                  <tr key={record.id}>
                    <td className="px-4 py-2 text-slate-700">{record.workDate}</td>
                    <td className="px-4 py-2 text-slate-700">{record.employeeName}</td>
                    <td className="px-4 py-2 text-slate-700">
                      {record.checkIn ? formatTime(record.checkIn) : "—"}
                    </td>
                    <td className="px-4 py-2 text-slate-700">
                      {record.checkOut ? (
                        formatTime(record.checkOut)
                      ) : incomplete ? (
                        <span className="rounded bg-amber-50 px-1.5 py-0.5 text-xs font-medium text-amber-700">
                          in progress
                        </span>
                      ) : (
                        "—"
                      )}
                    </td>
                    <td className="px-4 py-2 text-right text-slate-700">
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
