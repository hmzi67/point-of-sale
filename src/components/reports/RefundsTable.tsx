import { formatMinor } from "../../utils/format";
import type { RefundsSummary } from "../../types";

interface RefundsTableProps {
  data: RefundsSummary | null;
  currency: string;
  isLoading: boolean;
}

/** Receipt #, item(s) refunded, reason, processed by, date/time, amount —
 * already sorted most recent first server-side — with a visually distinct
 * grand total row, matching the style of the other report tables on this
 * screen (see `TableWiseSalesTable`). One row per refund; a refund with
 * several items lists them together in one cell rather than splitting into
 * multiple rows, since a receipt # only applies once per refund. */
export function RefundsTable({ data, currency, isLoading }: RefundsTableProps) {
  return (
    <div className="rounded-2xl border border-slate-200 bg-white shadow-soft">
      <div className="border-b border-slate-200 px-4 py-3">
        <h3 className="text-sm font-semibold text-slate-900">Refunds</h3>
        <p className="mt-0.5 text-xs text-slate-500">Every refund recorded in this range, most recent first.</p>
      </div>

      {isLoading ? (
        <div className="space-y-2 p-4">
          {Array.from({ length: 4 }).map((_, i) => (
            <div key={i} className="h-8 animate-pulse rounded bg-slate-50" />
          ))}
        </div>
      ) : !data || data.refunds.length === 0 ? (
        <p className="px-4 py-10 text-center text-sm text-slate-400">No refunds in this range</p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-left text-sm">
            <thead className="border-b border-slate-100 text-xs uppercase tracking-wide text-slate-500">
              <tr>
                <th className="px-4 py-2 font-medium">Receipt #</th>
                <th className="px-4 py-2 font-medium">Item(s)</th>
                <th className="px-4 py-2 font-medium">Reason</th>
                <th className="px-4 py-2 font-medium">Processed by</th>
                <th className="px-4 py-2 font-medium">Date</th>
                <th className="px-4 py-2 text-right font-medium">Amount</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-100">
              {data.refunds.map((refund) => (
                <tr key={refund.id}>
                  <td className="px-4 py-2 font-medium text-slate-900">#{refund.originalSaleId}</td>
                  <td className="max-w-xs px-4 py-2 text-slate-700">
                    {refund.items.map((line) => `${line.itemName} ×${line.qtyRefunded}`).join(", ")}
                  </td>
                  <td className="px-4 py-2 text-slate-500">{refund.reason ?? "—"}</td>
                  <td className="px-4 py-2 text-slate-500">{refund.refundedByName ?? "—"}</td>
                  <td className="px-4 py-2 whitespace-nowrap text-slate-500">{refund.createdAt}</td>
                  <td className="px-4 py-2 text-right text-slate-900">
                    {formatMinor(refund.totalRefundAmountMinor, currency)}
                  </td>
                </tr>
              ))}
            </tbody>
            <tfoot>
              <tr className="border-t-2 border-slate-300 bg-slate-50">
                <td className="px-4 py-2.5 text-sm font-bold text-slate-900" colSpan={5}>
                  Grand Total
                </td>
                <td className="px-4 py-2.5 text-right text-sm font-bold text-slate-900">
                  {formatMinor(data.grandTotalRefundedMinor, currency)}
                </td>
              </tr>
            </tfoot>
          </table>
        </div>
      )}
    </div>
  );
}
