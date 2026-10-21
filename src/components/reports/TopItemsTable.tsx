import { Trophy } from "lucide-react";
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
    <div className="overflow-x-auto rounded-2xl border border-line bg-surface">
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-line px-5 py-5">
        <div><h3 className="flex items-center gap-2 text-sm font-semibold text-fg"><Trophy className="h-4 w-4 text-warning-text" />Top-selling items</h3><p className="mt-1.5 text-xs text-fg-muted">Your standout products for this period</p></div>
        <div className="flex rounded-xl border border-line bg-surface-2/50 p-1">
          {SORT_OPTIONS.map((option) => (
            <button
              key={option.value}
              type="button"
              onClick={() => setTopItemsSort(option.value)}
              aria-pressed={topItemsSort === option.value}
              className={[
                "rounded-lg px-3 py-2 text-xs font-medium transition-colors",
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
        <table className="w-full min-w-[520px] text-left text-sm">
          <thead className="border-b border-line bg-surface-2/50 text-xs font-semibold text-fg-2">
            <tr>
              <th className="px-5 py-3.5 font-semibold">#</th>
              <th className="px-5 py-3.5 font-semibold">Item</th>
              <th className="px-5 py-3.5 text-right font-semibold">Qty sold</th>
              <th className="px-5 py-3.5 text-right font-semibold">Revenue</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-line">
            {items.map((item, index) => (
              <tr key={item.itemId} className="transition-colors hover:bg-surface-2/50">
                <td className="px-5 py-3.5 text-fg-muted"><span className={`flex h-7 w-7 items-center justify-center rounded-lg text-xs font-semibold ${index === 0 ? "bg-warning-soft text-warning-text" : "bg-surface-3 text-fg-muted"}`}>{index + 1}</span></td>
                <td className="px-5 py-3.5 font-medium text-fg">{item.itemName}</td>
                <td className="px-5 py-3.5 text-right text-fg-2">{formatQty(item.qtySold)}</td>
                <td className="px-5 py-3.5 whitespace-nowrap text-right font-bold text-fg tabular-nums">{formatMinor(item.revenueMinor, currency)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
