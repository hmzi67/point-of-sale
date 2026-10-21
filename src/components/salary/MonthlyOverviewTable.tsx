import { PersonAvatar } from "../ui/ManagementSummary";
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
    <div className="overflow-x-auto rounded-2xl border border-line bg-surface">
      <table className="min-w-full divide-y divide-line text-sm">
        <thead className="bg-surface-2/60 text-left text-xs font-semibold text-fg-2">
          <tr>
            <th className="px-4 py-3.5 font-semibold">Employee</th>
            <th className="px-4 py-3.5 text-right font-semibold">Days present</th>
            <th className="px-4 py-3.5 text-right font-semibold">Calculated</th>
            <th className="px-4 py-3.5 text-right font-semibold">Paid</th>
            <th className="px-4 py-3.5 font-semibold">Status</th>
            <th className="px-4 py-3.5" />
          </tr>
        </thead>
        <tbody className="divide-y divide-line">
          {isLoading ? (
            <tr>
              <td colSpan={6} className="px-4 py-12 text-center text-fg-muted">
                Loading…
              </td>
            </tr>
          ) : rows.length === 0 ? (
            <tr>
              <td colSpan={6} className="px-4 py-12 text-center text-fg-muted">
                No active employees on file.
              </td>
            </tr>
          ) : (
            rows.map((row) => (
              <tr key={row.employeeId} className="transition-colors hover:bg-surface-2/50">
                <td className="px-4 py-3.5 text-fg-2"><span className="flex items-center gap-3 font-medium text-fg"><PersonAvatar name={row.employeeName} />{row.employeeName}</span></td>
                <td className="px-4 py-3.5 whitespace-nowrap text-right text-fg-2 tabular-nums">
                  {row.daysPresent} / {row.workingDaysInMonth}
                </td>
                <td className="px-4 py-3.5 whitespace-nowrap text-right font-bold text-fg tabular-nums">
                  {formatMinor(row.calculatedAmountMinor, currency)}
                </td>
                <td className="px-4 py-3.5 whitespace-nowrap text-right font-bold text-fg tabular-nums">
                  {formatMinor(row.paidAmountMinor, currency)}
                </td>
                <td className="px-4 py-3.5">
                  <span className={`rounded-lg px-2.5 py-1 text-xs font-medium ${STATUS_STYLES[row.status]}`}>
                    {STATUS_LABEL[row.status]}
                  </span>
                </td>
                <td className="px-4 py-3.5 text-right">
                  {row.status !== "paid" && (
                    <button
                      type="button"
                      onClick={() => onRecordPayment(row)}
                      className="whitespace-nowrap rounded-lg border border-accent/20 bg-accent-soft px-3 py-2 text-xs font-semibold text-accent-text hover:bg-accent/20"
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
