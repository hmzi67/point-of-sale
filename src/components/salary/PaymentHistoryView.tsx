import { useEffect, useState } from "react";
import { useAppConfig } from "../../hooks/useAppConfig";
import { getEmployees } from "../../services/attendanceService";
import { getPaymentHistory } from "../../services/salaryService";
import { formatMinor } from "../../utils/format";
import type { Employee, PaymentStatus, SalaryCalculation } from "../../types";

const STATUS_STYLES: Record<PaymentStatus, string> = {
  paid: "bg-emerald-50 text-emerald-700",
  partial: "bg-amber-50 text-amber-700",
  unpaid: "bg-slate-100 text-slate-600",
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
        className="rounded-md border border-slate-300 px-2.5 py-1.5 text-sm"
      >
        {employees.map((employee) => (
          <option key={employee.id} value={employee.id}>
            {employee.name}
          </option>
        ))}
      </select>

      {error && <p className="rounded-md bg-red-50 px-3 py-2 text-sm text-red-700">{error}</p>}

      <div className="overflow-x-auto rounded-lg border border-slate-200 bg-white">
        <table className="min-w-full divide-y divide-slate-200 text-sm">
          <thead className="bg-slate-50 text-left text-xs font-medium uppercase tracking-wide text-slate-500">
            <tr>
              <th className="px-4 py-2">Month</th>
              <th className="px-4 py-2 text-right">Days present</th>
              <th className="px-4 py-2 text-right">Calculated</th>
              <th className="px-4 py-2 text-right">Paid</th>
              <th className="px-4 py-2">Paid on</th>
              <th className="px-4 py-2">Status</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-100">
            {isLoading ? (
              <tr>
                <td colSpan={6} className="px-4 py-6 text-center text-slate-400">
                  Loading…
                </td>
              </tr>
            ) : history.length === 0 ? (
              <tr>
                <td colSpan={6} className="px-4 py-6 text-center text-slate-400">
                  No salary records for this employee yet.
                </td>
              </tr>
            ) : (
              history.map((row) => (
                <tr key={row.month}>
                  <td className="px-4 py-2 text-slate-700">{row.month}</td>
                  <td className="px-4 py-2 text-right text-slate-700">
                    {row.daysPresent} / {row.workingDaysInMonth}
                  </td>
                  <td className="px-4 py-2 text-right text-slate-700">
                    {formatMinor(row.calculatedAmountMinor, config.currency)}
                  </td>
                  <td className="px-4 py-2 text-right text-slate-700">
                    {formatMinor(row.paidAmountMinor, config.currency)}
                  </td>
                  <td className="px-4 py-2 text-slate-500">{row.paidDate ?? "—"}</td>
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

