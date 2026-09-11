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
    <div className="rounded-2xl border border-line bg-surface shadow-soft">
      <div className="border-b border-line px-4 py-3">
        <h3 className="text-sm font-semibold text-fg">Table Wise Sales</h3>
        <p className="mt-0.5 text-xs text-fg-muted">Sales grouped by table, plus counter/takeaway sales.</p>
      </div>

      {isLoading ? (
        <div className="space-y-2 p-4">
          {Array.from({ length: 4 }).map((_, i) => (
            <div key={i} className="h-8 animate-pulse rounded bg-surface-2" />
          ))}
        </div>
      ) : !data || data.rows.length === 0 ? (
        <p className="px-4 py-10 text-center text-sm text-fg-muted">No sales in this range</p>
      ) : (
        <table className="w-full text-left text-sm">
          <thead className="border-b border-line text-xs uppercase tracking-wide text-fg-muted">
            <tr>
              <th className="px-4 py-2 font-medium">Table / Counter</th>
              <th className="px-4 py-2 text-right font-medium">Transactions</th>
              <th className="px-4 py-2 text-right font-medium">Amount</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-line">
            {data.rows.map((row) => (
              <tr key={row.tableId ?? "counter"}>
                <td className="px-4 py-2 font-medium text-fg">{row.label}</td>
                <td className="px-4 py-2 text-right text-fg-2">{row.transactionCount}</td>
                <td className="px-4 py-2 text-right text-fg">{formatMinor(row.totalMinor, currency)}</td>
              </tr>
            ))}
          </tbody>
          <tfoot>
            <tr className="border-t-2 border-line-strong bg-surface-2">
              <td className="px-4 py-2.5 text-sm font-bold text-fg">Grand Total</td>
              <td />
              <td className="px-4 py-2.5 text-right text-sm font-bold text-fg">
                {formatMinor(data.grandTotalMinor, currency)}
              </td>
            </tr>
          </tfoot>
        </table>
      )}
    </div>
  );
}
