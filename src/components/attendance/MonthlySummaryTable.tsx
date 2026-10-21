import { useEffect, useState } from "react";
import { getMonthlySummary } from "../../services/attendanceService";
import { formatHours } from "../../utils/format";
import type { MonthlySummary } from "../../types";
import { currentMonth } from "../../utils/dateRanges";

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
        className="rounded-xl border border-line-strong px-3 py-2.5 text-sm"
      />

      {error && <p className="rounded-xl bg-danger-soft px-3 py-2 text-sm text-danger-text">{error}</p>}

      <div className="overflow-x-auto rounded-2xl border border-line bg-surface">
        <table className="min-w-full divide-y divide-line text-sm">
          <thead className="bg-surface-2/60 text-left text-xs font-semibold text-fg-2">
            <tr>
              <th className="px-4 py-3.5 font-semibold">Employee</th>
              <th className="px-4 py-3.5 text-right font-semibold">Days present</th>
              <th className="px-4 py-3.5 text-right font-semibold">Days absent</th>
              <th className="px-4 py-3.5 text-right font-semibold">Total hours</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-line">
            {isLoading ? (
              <tr>
                <td colSpan={4} className="px-4 py-12 text-center text-fg-muted">
                  Loading…
                </td>
              </tr>
            ) : rows.length === 0 ? (
              <tr>
                <td colSpan={4} className="px-4 py-12 text-center text-fg-muted">
                  No employees on file.
                </td>
              </tr>
            ) : (
              rows.map((row) => (
                <tr key={row.employeeId} className="transition-colors hover:bg-surface-2/50">
                  <td className="px-4 py-3.5 text-fg-2">{row.employeeName}</td>
                  <td className="px-4 py-3.5 whitespace-nowrap text-right text-fg-2 tabular-nums">
                    {row.daysPresent}
                    {row.incompleteDays > 0 && (
                      <span
                        title={`${row.incompleteDays} shift${row.incompleteDays === 1 ? "" : "s"} missing a check-out`}
                        className="ml-1.5 rounded-lg bg-warning-soft px-2.5 py-1 text-xs font-medium text-warning-text"
                      >
                        {row.incompleteDays} incomplete
                      </span>
                    )}
                  </td>
                  <td className="px-4 py-3.5 whitespace-nowrap text-right text-fg-2 tabular-nums">{row.daysAbsent}</td>
                  <td className="px-4 py-3.5 whitespace-nowrap text-right text-fg-2 tabular-nums">{formatHours(row.totalHours)}</td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
