import { useReportsStore } from "../../store";
import { formatMinor, formatQty } from "../../utils/format";
import type { TopItem, TopItemSort } from "../../types";

interface TopItemsTableProps {
  items: TopItem[];
  currency: string;
  isLoading: boolean;
}

const SORT_OPTIONS: { value: TopItemSort; label: string }[] = [
  { value: "quantity", label: "By quantity" },
  { value: "revenue", label: "By revenue" },
];

export function TopItemsTable({ items, currency, isLoading }: TopItemsTableProps) {
  const topItemsSort = useReportsStore((state) => state.topItemsSort);
  const setTopItemsSort = useReportsStore((state) => state.setTopItemsSort);

  return (
    <div className="rounded-2xl border border-line bg-surface shadow-soft">
      <div className="flex items-center justify-between border-b border-line px-4 py-3">
        <h3 className="text-sm font-semibold text-fg">Top-selling items</h3>
        <div className="flex rounded-md border border-line-strong p-0.5">
          {SORT_OPTIONS.map((option) => (
            <button
              key={option.value}
              type="button"
              onClick={() => setTopItemsSort(option.value)}
              className={[
                "rounded px-2.5 py-1 text-xs font-medium transition-colors",
                topItemsSort === option.value ? "bg-accent text-accent-ink" : "text-fg-muted hover:bg-surface-3",
              ].join(" ")}
            >
              {option.label}
            </button>
          ))}
        </div>
      </div>

      {isLoading ? (
        <div className="space-y-2 p-4">
          {Array.from({ length: 4 }).map((_, i) => (
            <div key={i} className="h-8 animate-pulse rounded bg-surface-2" />
          ))}
        </div>
      ) : items.length === 0 ? (
        <p className="px-4 py-10 text-center text-sm text-fg-muted">No sales in this range</p>
      ) : (
        <table className="w-full text-left text-sm">
          <thead className="border-b border-line text-xs uppercase tracking-wide text-fg-muted">
            <tr>
              <th className="px-4 py-2 font-medium">#</th>
              <th className="px-4 py-2 font-medium">Item</th>
              <th className="px-4 py-2 text-right font-medium">Qty sold</th>
              <th className="px-4 py-2 text-right font-medium">Revenue</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-line">
            {items.map((item, index) => (
              <tr key={item.itemId}>
                <td className="px-4 py-2 text-fg-muted">{index + 1}</td>
                <td className="px-4 py-2 font-medium text-fg">{item.itemName}</td>
                <td className="px-4 py-2 text-right text-fg-2">{formatQty(item.qtySold)}</td>
                <td className="px-4 py-2 text-right text-fg">{formatMinor(item.revenueMinor, currency)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
