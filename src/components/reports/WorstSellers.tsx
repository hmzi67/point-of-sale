import type { ProductSalesSummaryReport } from "../../types";
import { formatMinor, formatQty } from "../../utils/format";

/** The slowest movers in the range: items that never sold first, then the
 * lowest revenue among those that did. */
export function WorstSellers({ data, currency }: { data: ProductSalesSummaryReport | null; currency: string }) {
  if (!data) return null;
  const sold = [...data.rows].sort((a, b) => a.revenueMinor - b.revenueMinor).slice(0, 10);
  return (
    <div className="rounded-2xl border border-line bg-surface p-5">
      <h3 className="text-sm font-semibold text-fg">Worst sellers</h3>
      <p className="mt-1 text-xs text-fg-muted">
        {data.noSalesItems.length} active item(s) didn't sell at all in this period
        {data.noSalesItems.length > 0 && `: ${data.noSalesItems.slice(0, 8).map((i) => i.itemName).join(", ")}${data.noSalesItems.length > 8 ? "…" : ""}`}
      </p>
      {sold.length > 0 && (
        <ul className="mt-3 divide-y divide-line text-sm">
          {sold.map((row) => (
            <li key={row.itemId} className="flex justify-between py-1.5 tabular-nums">
              <span className="text-fg-2">{row.itemName} <span className="text-xs text-fg-muted">× {formatQty(row.qtySold)}</span></span>
              <span className="text-fg">{formatMinor(row.revenueMinor, currency)}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
