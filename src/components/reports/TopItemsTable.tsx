import { useReportsStore } from "../../store";
import { formatMinor } from "../../utils/format";
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
    <div className="rounded-lg border border-slate-200 bg-white">
      <div className="flex items-center justify-between border-b border-slate-200 px-4 py-3">
        <h3 className="text-sm font-semibold text-slate-900">Top-selling items</h3>
        <div className="flex rounded-md border border-slate-300 p-0.5">
          {SORT_OPTIONS.map((option) => (
            <button
              key={option.value}
              type="button"
              onClick={() => setTopItemsSort(option.value)}
              className={[
                "rounded px-2.5 py-1 text-xs font-medium transition-colors",
                topItemsSort === option.value ? "bg-brand-600 text-white" : "text-slate-500 hover:bg-slate-100",
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
            <div key={i} className="h-8 animate-pulse rounded bg-slate-50" />
          ))}
        </div>
      ) : items.length === 0 ? (
        <p className="px-4 py-10 text-center text-sm text-slate-400">No sales in this range</p>
      ) : (
        <table className="w-full text-left text-sm">
          <thead className="border-b border-slate-100 text-xs uppercase tracking-wide text-slate-500">
            <tr>
              <th className="px-4 py-2 font-medium">#</th>
              <th className="px-4 py-2 font-medium">Item</th>
              <th className="px-4 py-2 text-right font-medium">Qty sold</th>
              <th className="px-4 py-2 text-right font-medium">Revenue</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-100">
            {items.map((item, index) => (
              <tr key={item.itemId}>
                <td className="px-4 py-2 text-slate-400">{index + 1}</td>
                <td className="px-4 py-2 font-medium text-slate-900">{item.itemName}</td>
                <td className="px-4 py-2 text-right text-slate-700">{item.qtySold}</td>
                <td className="px-4 py-2 text-right text-slate-900">{formatMinor(item.revenueMinor, currency)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
