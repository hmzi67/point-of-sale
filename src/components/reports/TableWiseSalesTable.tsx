import { formatMinor } from "../../utils/format";
import type { TableSalesSummary } from "../../types";

interface TableWiseSalesTableProps {
  data: TableSalesSummary | null;
  currency: string;
  isLoading: boolean;
}

/** Table/counter, amount, transaction count — sorted by amount descending
 * (already sorted server-side), with a visually distinct grand total row,
 * matching the style of the other report totals in this screen. */
export function TableWiseSalesTable({ data, currency, isLoading }: TableWiseSalesTableProps) {
  return (
    <div className="rounded-2xl border border-slate-200 bg-white shadow-soft">
      <div className="border-b border-slate-200 px-4 py-3">
        <h3 className="text-sm font-semibold text-slate-900">Table Wise Sales</h3>
        <p className="mt-0.5 text-xs text-slate-500">Sales grouped by table, plus counter/takeaway sales.</p>
      </div>

      {isLoading ? (
        <div className="space-y-2 p-4">
          {Array.from({ length: 4 }).map((_, i) => (
            <div key={i} className="h-8 animate-pulse rounded bg-slate-50" />
          ))}
        </div>
      ) : !data || data.rows.length === 0 ? (
        <p className="px-4 py-10 text-center text-sm text-slate-400">No sales in this range</p>
      ) : (
        <table className="w-full text-left text-sm">
          <thead className="border-b border-slate-100 text-xs uppercase tracking-wide text-slate-500">
            <tr>
              <th className="px-4 py-2 font-medium">Table / Counter</th>
              <th className="px-4 py-2 text-right font-medium">Transactions</th>
              <th className="px-4 py-2 text-right font-medium">Amount</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-100">
            {data.rows.map((row) => (
              <tr key={row.tableId ?? "counter"}>
                <td className="px-4 py-2 font-medium text-slate-900">{row.label}</td>
                <td className="px-4 py-2 text-right text-slate-700">{row.transactionCount}</td>
                <td className="px-4 py-2 text-right text-slate-900">{formatMinor(row.totalMinor, currency)}</td>
              </tr>
            ))}
          </tbody>
          <tfoot>
            <tr className="border-t-2 border-slate-300 bg-slate-50">
              <td className="px-4 py-2.5 text-sm font-bold text-slate-900">Grand Total</td>
              <td />
              <td className="px-4 py-2.5 text-right text-sm font-bold text-slate-900">
                {formatMinor(data.grandTotalMinor, currency)}
              </td>
            </tr>
          </tfoot>
        </table>
      )}
    </div>
  );
}
