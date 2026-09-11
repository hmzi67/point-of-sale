import { formatMinor } from "../../utils/format";
import type { PaymentStatus, SalaryCalculation } from "../../types";

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
    <div className="overflow-x-auto rounded-lg border border-line bg-surface">
      <table className="min-w-full divide-y divide-line text-sm">
        <thead className="bg-surface-2 text-left text-xs font-medium uppercase tracking-wide text-fg-muted">
          <tr>
            <th className="px-4 py-2">Employee</th>
            <th className="px-4 py-2 text-right">Days present</th>
            <th className="px-4 py-2 text-right">Calculated</th>
            <th className="px-4 py-2 text-right">Paid</th>
            <th className="px-4 py-2">Status</th>
            <th className="px-4 py-2" />
          </tr>
        </thead>
        <tbody className="divide-y divide-line">
          {isLoading ? (
            <tr>
              <td colSpan={6} className="px-4 py-6 text-center text-fg-muted">
                Loading…
              </td>
            </tr>
          ) : rows.length === 0 ? (
            <tr>
              <td colSpan={6} className="px-4 py-6 text-center text-fg-muted">
                No active employees on file.
              </td>
            </tr>
          ) : (
            rows.map((row) => (
              <tr key={row.employeeId}>
                <td className="px-4 py-2 text-fg-2">{row.employeeName}</td>
                <td className="px-4 py-2 text-right text-fg-2">
                  {row.daysPresent} / {row.workingDaysInMonth}
                </td>
                <td className="px-4 py-2 text-right text-fg-2">
                  {formatMinor(row.calculatedAmountMinor, currency)}
                </td>
                <td className="px-4 py-2 text-right text-fg-2">
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
                      className="rounded-md border border-line-strong px-2.5 py-1 text-xs font-medium text-fg-2 hover:bg-surface-2"
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
