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
    <div className="overflow-x-auto rounded-lg border border-slate-200 bg-white">
      <table className="min-w-full divide-y divide-slate-200 text-sm">
        <thead className="bg-slate-50 text-left text-xs font-medium uppercase tracking-wide text-slate-500">
          <tr>
            <th className="px-4 py-2">Date</th>
            <th className="px-4 py-2">Category</th>
            <th className="px-4 py-2">Note</th>
            <th className="px-4 py-2 text-right">Amount</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-slate-100">
          {isLoading ? (
            <tr>
              <td colSpan={4} className="px-4 py-6 text-center text-slate-400">
                Loading…
              </td>
            </tr>
          ) : expenses.length === 0 ? (
            <tr>
              <td colSpan={4} className="px-4 py-6 text-center text-slate-400">
                No expenses in this range.
              </td>
            </tr>
          ) : (
            expenses.map((expense) => (
              <tr key={expense.id}>
                <td className="px-4 py-2 text-slate-700">{expense.expenseDate}</td>
                <td className="px-4 py-2">
                  <span className="rounded bg-slate-100 px-1.5 py-0.5 text-xs font-medium text-slate-600">
                    {expense.category}
                  </span>
                </td>
                <td className="px-4 py-2 text-slate-500">{expense.note ?? "—"}</td>
                <td className="px-4 py-2 text-right font-medium text-slate-900">
                  {formatMinor(expense.amountMinor, currency)}
                </td>
              </tr>
            ))
          )}
        </tbody>
        {expenses.length > 0 && (
          <tfoot className="border-t border-slate-200 bg-slate-50">
            <tr>
              <td colSpan={3} className="px-4 py-2 text-right text-xs font-medium uppercase tracking-wide text-slate-500">
                Total
              </td>
              <td className="px-4 py-2 text-right font-semibold text-slate-900">
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
