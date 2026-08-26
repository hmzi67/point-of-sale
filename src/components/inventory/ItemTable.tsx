import { Flame, Pencil, Trash2 } from "lucide-react";
import { formatMinor, formatQty } from "../../utils/format";
import { ItemThumbnail } from "./ItemThumbnail";
import type { Item } from "../../types";

interface ItemTableProps {
  items: Item[];
  isReadOnly: boolean;
  currency: string;
  onEdit: (item: Item) => void;
  onDelete: (item: Item) => void;
  /** Selection is optional — omit both props to render the table with no
   * checkbox column at all (kept for any other place `ItemTable` might be
   * reused without multi-select). */
  selectedIds?: Set<number>;
  onToggleSelect?: (id: number) => void;
  onToggleSelectAll?: () => void;
  /** Item ids currently qualifying as a "best seller" (see
   * `useBestSellerIds`) — badged with a small fire icon next to the name. */
  bestSellerIds?: Set<number>;
}

export function ItemTable({
  items,
  isReadOnly,
  currency,
  onEdit,
  onDelete,
  selectedIds,
  onToggleSelect,
  onToggleSelectAll,
  bestSellerIds,
}: ItemTableProps) {
  const canSelect = !isReadOnly && !!selectedIds && !!onToggleSelect && !!onToggleSelectAll;
  const allSelected = canSelect && items.length > 0 && items.every((item) => selectedIds!.has(item.id));
  const someSelected = canSelect && !allSelected && items.some((item) => selectedIds!.has(item.id));

  if (items.length === 0) {
    return (
      <div className="rounded-lg border border-dashed border-slate-300 bg-white py-16 text-center text-sm text-slate-500">
        No items match your search.
      </div>
    );
  }

  return (
    <div className="overflow-x-auto rounded-lg border border-slate-200 bg-white">
      <table className="w-full min-w-[720px] text-left text-sm">
        <thead className="border-b border-slate-200 bg-slate-50 text-xs uppercase tracking-wide text-slate-500">
          <tr>
            {canSelect && (
              <th className="w-10 px-4 py-3" aria-hidden="false">
                <input
                  type="checkbox"
                  checked={allSelected}
                  ref={(el) => {
                    if (el) el.indeterminate = someSelected;
                  }}
                  onChange={onToggleSelectAll}
                  aria-label="Select all items"
                  className="h-4 w-4 rounded border-slate-300 text-brand-600 focus:ring-brand-500"
                />
              </th>
            )}
            <th className="w-12 px-4 py-3 font-medium" aria-hidden="true"></th>
            <th className="px-4 py-3 font-medium">Item</th>
            <th className="px-4 py-3 font-medium">Barcode</th>
            <th className="px-4 py-3 font-medium">Category</th>
            <th className="px-4 py-3 text-right font-medium">Price</th>
            <th className="px-4 py-3 text-right font-medium">Cost</th>
            <th className="px-4 py-3 text-right font-medium">Stock</th>
            {!isReadOnly && <th className="px-4 py-3 text-right font-medium">Actions</th>}
          </tr>
        </thead>
        <tbody className="divide-y divide-slate-100">
          {items.map((item) => {
            const isSelected = !!selectedIds?.has(item.id);
            return (
              <tr key={item.id} className={isSelected ? "bg-brand-50/60" : "hover:bg-slate-50"}>
                {canSelect && (
                  <td className="px-4 py-3">
                    <input
                      type="checkbox"
                      checked={isSelected}
                      onChange={() => onToggleSelect!(item.id)}
                      aria-label={`Select ${item.name}`}
                      className="h-4 w-4 rounded border-slate-300 text-brand-600 focus:ring-brand-500"
                    />
                  </td>
                )}
                <td className="px-4 py-3">
                  <ItemThumbnail imagePath={item.imagePath} name={item.name} />
                </td>
                <td className="px-4 py-3 font-medium text-slate-900">
                  <span className="inline-flex items-center gap-1.5">
                    {item.name}
                    {bestSellerIds?.has(item.id) && (
                      <span title="Best seller">
                        <Flame className="h-3.5 w-3.5 shrink-0 text-orange-500" aria-label="Best seller" />
                      </span>
                    )}
                  </span>
                </td>
                <td className="px-4 py-3 font-mono text-xs text-slate-500">{item.barcode ?? "—"}</td>
                <td className="px-4 py-3 text-slate-600">{item.categoryName ?? "Uncategorized"}</td>
                <td className="px-4 py-3 text-right text-slate-900">{formatMinor(item.priceMinor, currency)}</td>
                <td className="px-4 py-3 text-right text-slate-500">{formatMinor(item.costMinor, currency)}</td>
                <td className="px-4 py-3 text-right">
                  <span
                    className={[
                      "inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 font-medium",
                      item.isLowStock ? "bg-red-100 text-red-700" : "text-slate-700",
                    ].join(" ")}
                  >
                    {item.isLowStock && <span className="h-1.5 w-1.5 rounded-full bg-red-600" />}
                    {formatQty(item.stockQty, item.unit)}
                  </span>
                </td>
                {!isReadOnly && (
                  <td className="px-4 py-3">
                    <div className="flex justify-end gap-1">
                      <button
                        type="button"
                        onClick={() => onEdit(item)}
                        className="rounded-md p-1.5 text-slate-500 hover:bg-slate-100 hover:text-slate-900"
                        aria-label={`Edit ${item.name}`}
                      >
                        <Pencil className="h-4 w-4" />
                      </button>
                      <button
                        type="button"
                        onClick={() => onDelete(item)}
                        className="rounded-md p-1.5 text-slate-500 hover:bg-red-50 hover:text-red-600"
                        aria-label={`Delete ${item.name}`}
                      >
                        <Trash2 className="h-4 w-4" />
                      </button>
                    </div>
                  </td>
                )}
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
