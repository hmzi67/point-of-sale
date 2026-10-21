import { formatMinor } from "../../utils/format";
import type { Expense } from "../../types";

interface ExpenseListProps {
  expenses: Expense[];
  currency: string;
  isLoading: boolean;
}

/** The filtered expense log — filtering itself (date range + category) lives
 * in `ExpensesPage`, shared with `CategoryBreakdown` so both views always
 * agree on what range they're looking at. */
export function ExpenseList({ expenses, currency, isLoading }: ExpenseListProps) {
  return (
    <div className="overflow-x-auto rounded-2xl border border-line bg-surface">
      <table className="min-w-full divide-y divide-line text-sm">
        <thead className="bg-surface-2/60 text-left text-xs font-semibold text-fg-2">
          <tr>
            <th className="px-4 py-3.5 font-semibold">Date</th>
            <th className="px-4 py-3.5 font-semibold">Category</th>
            <th className="px-4 py-3.5 font-semibold">Note</th>
            <th className="px-4 py-3.5 text-right font-semibold">Amount</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-line">
          {isLoading ? (
            <tr>
              <td colSpan={4} className="px-4 py-12 text-center text-fg-muted">
                Loading…
              </td>
            </tr>
          ) : expenses.length === 0 ? (
            <tr>
              <td colSpan={4} className="px-4 py-12 text-center text-fg-muted">
                No expenses in this range.
              </td>
            </tr>
          ) : (
            expenses.map((expense) => (
              <tr key={expense.id} className="transition-colors hover:bg-surface-2/50">
                <td className="px-4 py-3.5 text-fg-2">{expense.expenseDate}</td>
                <td className="px-4 py-3.5">
                  <span className="rounded bg-surface-3 px-1.5 py-0.5 text-xs font-medium text-fg-2">
                    {expense.category}
                  </span>
                </td>
                <td className="px-4 py-3.5 text-fg-muted">{expense.note ?? "—"}</td>
                <td className="px-4 py-3.5 whitespace-nowrap text-right font-bold text-fg tabular-nums">
                  {formatMinor(expense.amountMinor, currency)}
                </td>
              </tr>
            ))
          )}
        </tbody>
        {expenses.length > 0 && (
          <tfoot className="border-t border-line bg-surface-2">
            <tr>
              <td colSpan={3} className="px-4 py-3.5 text-right text-xs font-semibold text-fg-2">
                Total
              </td>
              <td className="px-4 py-3.5 text-right font-bold tabular-nums text-fg">
                {formatMinor(
                  expenses.reduce((sum, e) => sum + e.amountMinor, 0),
                  currency,
                )}
              </td>
            </tr>
          </tfoot>
        )}
      </table>
    </div>
  );
}
