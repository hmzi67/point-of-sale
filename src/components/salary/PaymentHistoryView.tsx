import { useEffect, useState } from "react";
import { useAppConfig } from "../../hooks/useAppConfig";
import { getEmployees } from "../../services/attendanceService";
import { getPaymentHistory } from "../../services/salaryService";
import { formatMinor } from "../../utils/format";
import type { Employee, PaymentStatus, SalaryCalculation } from "../../types";

const STATUS_STYLES: Record<PaymentStatus, string> = {
  paid: "bg-success-soft text-success-text",
  partial: "bg-warning-soft text-warning-text",
  unpaid: "bg-surface-3 text-fg-2",
};

const STATUS_LABEL: Record<PaymentStatus, string> = {
  paid: "Paid",
  partial: "Partial",
  unpaid: "Unpaid",
};

/** Past months' salary and payments for one employee at a time. */
export function PaymentHistoryView() {
  const [employees, setEmployees] = useState<Employee[]>([]);
  const [employeeId, setEmployeeId] = useState<number | null>(null);
  const [history, setHistory] = useState<SalaryCalculation[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const { config } = useAppConfig();

  useEffect(() => {
    getEmployees()
      .then((staff) => {
        setEmployees(staff);
        setEmployeeId((current) => current ?? staff[0]?.id ?? null);
      })
      .catch((e: Error) => setError(e.message));
  }, []);

  useEffect(() => {
    if (employeeId === null) {
      setIsLoading(false);
      return;
    }
    setIsLoading(true);
    setError(null);
    getPaymentHistory(employeeId)
      .then(setHistory)
      .catch((e: Error) => setError(e.message))
      .finally(() => setIsLoading(false));
  }, [employeeId]);

  return (
    <div className="space-y-3">
      <select
        value={employeeId ?? ""}
        onChange={(e) => setEmployeeId(e.target.value ? Number(e.target.value) : null)}
        className="rounded-md border border-line-strong px-2.5 py-1.5 text-sm"
      >
        {employees.map((employee) => (
          <option key={employee.id} value={employee.id}>
            {employee.name}
          </option>
        ))}
      </select>

      {error && <p className="rounded-md bg-danger-soft px-3 py-2 text-sm text-danger-text">{error}</p>}

      <div className="overflow-x-auto rounded-lg border border-line bg-surface">
        <table className="min-w-full divide-y divide-line text-sm">
          <thead className="bg-surface-2 text-left text-xs font-medium uppercase tracking-wide text-fg-muted">
            <tr>
              <th className="px-4 py-2">Month</th>
              <th className="px-4 py-2 text-right">Days present</th>
              <th className="px-4 py-2 text-right">Calculated</th>
              <th className="px-4 py-2 text-right">Paid</th>
              <th className="px-4 py-2">Paid on</th>
              <th className="px-4 py-2">Status</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-line">
            {isLoading ? (
              <tr>
                <td colSpan={6} className="px-4 py-6 text-center text-fg-muted">
                  Loading…
                </td>
              </tr>
            ) : history.length === 0 ? (
              <tr>
                <td colSpan={6} className="px-4 py-6 text-center text-fg-muted">
                  No salary records for this employee yet.
                </td>
              </tr>
            ) : (
              history.map((row) => (
                <tr key={row.month}>
                  <td className="px-4 py-2 text-fg-2">{row.month}</td>
                  <td className="px-4 py-2 text-right text-fg-2">
                    {row.daysPresent} / {row.workingDaysInMonth}
                  </td>
                  <td className="px-4 py-2 text-right text-fg-2">
                    {formatMinor(row.calculatedAmountMinor, config.currency)}
                  </td>
                  <td className="px-4 py-2 text-right text-fg-2">
                    {formatMinor(row.paidAmountMinor, config.currency)}
                  </td>
                  <td className="px-4 py-2 text-fg-muted">{row.paidDate ?? "—"}</td>
                  <td className="px-4 py-2">
                    <span className={`rounded px-1.5 py-0.5 text-xs font-medium ${STATUS_STYLES[row.status]}`}>
                      {STATUS_LABEL[row.status]}
                    </span>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}

