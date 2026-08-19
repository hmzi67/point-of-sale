import { useEffect, useState } from "react";
import { getMonthlySummary } from "../../services/attendanceService";
import { formatHours } from "../../utils/format";
import type { MonthlySummary } from "../../types";

function currentMonth(): string {
  const now = new Date();
  return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}`;
}

/** One row per employee: days present, days absent and total hours for a
 * month — the feed Phase 9 (Salary) will read from. A day with a check-in
 * but no check-out still counts toward `daysPresent` (the employee showed
 * up) but is called out via the "incomplete" badge rather than silently
 * contributing zero hours. */
export function MonthlySummaryTable() {
  const [month, setMonth] = useState(currentMonth());
  const [rows, setRows] = useState<MonthlySummary[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setIsLoading(true);
    setError(null);
    getMonthlySummary(month)
      .then(setRows)
      .catch((e: Error) => setError(e.message))
      .finally(() => setIsLoading(false));
  }, [month]);

  return (
    <div className="space-y-3">
      <input
        type="month"
        value={month}
        onChange={(e) => setMonth(e.target.value)}
        className="rounded-md border border-slate-300 px-2.5 py-1.5 text-sm"
      />

      {error && <p className="rounded-md bg-red-50 px-3 py-2 text-sm text-red-700">{error}</p>}

      <div className="overflow-x-auto rounded-lg border border-slate-200 bg-white">
        <table className="min-w-full divide-y divide-slate-200 text-sm">
          <thead className="bg-slate-50 text-left text-xs font-medium uppercase tracking-wide text-slate-500">
            <tr>
              <th className="px-4 py-2">Employee</th>
              <th className="px-4 py-2 text-right">Days present</th>
              <th className="px-4 py-2 text-right">Days absent</th>
              <th className="px-4 py-2 text-right">Total hours</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-100">
            {isLoading ? (
              <tr>
                <td colSpan={4} className="px-4 py-6 text-center text-slate-400">
                  Loading…
                </td>
              </tr>
            ) : rows.length === 0 ? (
              <tr>
                <td colSpan={4} className="px-4 py-6 text-center text-slate-400">
                  No employees on file.
                </td>
              </tr>
            ) : (
              rows.map((row) => (
                <tr key={row.employeeId}>
                  <td className="px-4 py-2 text-slate-700">{row.employeeName}</td>
                  <td className="px-4 py-2 text-right text-slate-700">
                    {row.daysPresent}
                    {row.incompleteDays > 0 && (
                      <span
                        title={`${row.incompleteDays} shift${row.incompleteDays === 1 ? "" : "s"} missing a check-out`}
                        className="ml-1.5 rounded bg-amber-50 px-1.5 py-0.5 text-xs font-medium text-amber-700"
                      >
                        {row.incompleteDays} incomplete
                      </span>
                    )}
                  </td>
                  <td className="px-4 py-2 text-right text-slate-700">{row.daysAbsent}</td>
                  <td className="px-4 py-2 text-right text-slate-700">{formatHours(row.totalHours)}</td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
