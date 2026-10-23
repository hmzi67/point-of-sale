import { useEffect, useState } from "react";
import { useAppConfig } from "../../hooks/useAppConfig";
import { getEmployees } from "../../services/attendanceService";
import { getPaymentHistory } from "../../services/salaryService";
import { formatMinor } from "../../utils/format";
import type { Employee, PaymentStatus, SalaryCalculation } from "../../types";
import { useReport } from "../../hooks/useReport";

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
  const [employeesError, setEmployeesError] = useState<string | null>(null);
  const { config } = useAppConfig();

  useEffect(() => {
    getEmployees()
      .then((staff) => {
        setEmployees(staff);
        setEmployeeId((current) => current ?? staff[0]?.id ?? null);
      })
      .catch((e: Error) => setEmployeesError(e.message));
  }, []);

  const payments = useReport(() => getPaymentHistory(employeeId as number), [employeeId], { enabled: employeeId !== null });
  const history: SalaryCalculation[] = payments.data ?? [];
  const isLoading = payments.isLoading;
  const error = payments.error ?? employeesError;

  return (
    <div className="space-y-3">
      <select
        value={employeeId ?? ""}
        onChange={(e) => setEmployeeId(e.target.value ? Number(e.target.value) : null)}
        className="rounded-xl border border-line-strong px-3 py-2.5 text-sm"
      >
        {employees.map((employee) => (
          <option key={employee.id} value={employee.id}>
            {employee.name}
          </option>
        ))}
      </select>

      {error && <p className="rounded-xl bg-danger-soft px-3 py-2 text-sm text-danger-text">{error}</p>}

      <div className="overflow-x-auto rounded-2xl border border-line bg-surface">
        <table className="min-w-full divide-y divide-line text-sm">
          <thead className="bg-surface-2/60 text-left text-xs font-semibold text-fg-2">
            <tr>
              <th className="px-4 py-3.5 font-semibold">Month</th>
              <th className="px-4 py-3.5 text-right font-semibold">Days present</th>
              <th className="px-4 py-3.5 text-right font-semibold">Calculated</th>
              <th className="px-4 py-3.5 text-right font-semibold">Paid</th>
              <th className="px-4 py-3.5 font-semibold">Paid on</th>
              <th className="px-4 py-3.5 font-semibold">Status</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-line">
            {isLoading ? (
              <tr>
                <td colSpan={6} className="px-4 py-12 text-center text-fg-muted">
                  Loading…
                </td>
              </tr>
            ) : history.length === 0 ? (
              <tr>
                <td colSpan={6} className="px-4 py-12 text-center text-fg-muted">
                  No salary records for this employee yet.
                </td>
              </tr>
            ) : (
              history.map((row) => (
                <tr key={row.month} className="transition-colors hover:bg-surface-2/50">
                  <td className="px-4 py-3.5 text-fg-2">{row.month}</td>
                  <td className="px-4 py-3.5 whitespace-nowrap text-right text-fg-2 tabular-nums">
                    {row.daysPresent} / {row.workingDaysInMonth}
                  </td>
                  <td className="px-4 py-3.5 whitespace-nowrap text-right font-bold text-fg tabular-nums">
                    {formatMinor(row.calculatedAmountMinor, config.currency)}
                  </td>
                  <td className="px-4 py-3.5 whitespace-nowrap text-right font-bold text-fg tabular-nums">
                    {formatMinor(row.paidAmountMinor, config.currency)}
                  </td>
                  <td className="px-4 py-3.5 text-fg-muted">{row.paidDate ?? "—"}</td>
                  <td className="px-4 py-3.5">
                    <span className={`rounded-lg px-2.5 py-1 text-xs font-medium ${STATUS_STYLES[row.status]}`}>
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

