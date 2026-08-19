import { formatMinor } from "../../utils/format";
import type { PaymentStatus, SalaryCalculation } from "../../types";

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

interface MonthlyOverviewTableProps {
  rows: SalaryCalculation[];
  currency: string;
  isLoading: boolean;
  onRecordPayment: (row: SalaryCalculation) => void;
}

/** One row per active employee for the selected month: what attendance says
 * they're owed (`calculatedAmountMinor`, live off Phase 7's monthly summary),
 * what's actually gone out (`paidAmountMinor`), and a status derived from
 * comparing the two. */
export function MonthlyOverviewTable({ rows, currency, isLoading, onRecordPayment }: MonthlyOverviewTableProps) {
  return (
    <div className="overflow-x-auto rounded-lg border border-slate-200 bg-white">
      <table className="min-w-full divide-y divide-slate-200 text-sm">
        <thead className="bg-slate-50 text-left text-xs font-medium uppercase tracking-wide text-slate-500">
          <tr>
            <th className="px-4 py-2">Employee</th>
            <th className="px-4 py-2 text-right">Days present</th>
            <th className="px-4 py-2 text-right">Calculated</th>
            <th className="px-4 py-2 text-right">Paid</th>
            <th className="px-4 py-2">Status</th>
            <th className="px-4 py-2" />
          </tr>
        </thead>
        <tbody className="divide-y divide-slate-100">
          {isLoading ? (
            <tr>
              <td colSpan={6} className="px-4 py-6 text-center text-slate-400">
                Loading…
              </td>
            </tr>
          ) : rows.length === 0 ? (
            <tr>
              <td colSpan={6} className="px-4 py-6 text-center text-slate-400">
                No active employees on file.
              </td>
            </tr>
          ) : (
            rows.map((row) => (
              <tr key={row.employeeId}>
                <td className="px-4 py-2 text-slate-700">{row.employeeName}</td>
                <td className="px-4 py-2 text-right text-slate-700">
                  {row.daysPresent} / {row.workingDaysInMonth}
                </td>
                <td className="px-4 py-2 text-right text-slate-700">
                  {formatMinor(row.calculatedAmountMinor, currency)}
                </td>
                <td className="px-4 py-2 text-right text-slate-700">
                  {formatMinor(row.paidAmountMinor, currency)}
                </td>
                <td className="px-4 py-2">
                  <span className={`rounded px-1.5 py-0.5 text-xs font-medium ${STATUS_STYLES[row.status]}`}>
                    {STATUS_LABEL[row.status]}
                  </span>
                </td>
                <td className="px-4 py-2 text-right">
                  {row.status !== "paid" && (
                    <button
                      type="button"
                      onClick={() => onRecordPayment(row)}
                      className="rounded-md border border-slate-300 px-2.5 py-1 text-xs font-medium text-slate-700 hover:bg-slate-50"
                    >
                      Record payment
                    </button>
                  )}
                </td>
              </tr>
            ))
          )}
        </tbody>
      </table>
    </div>
  );
}
