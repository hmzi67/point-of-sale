import { Pencil, Trash2 } from "lucide-react";
import { formatMinor } from "../../utils/format";
import { ItemThumbnail } from "./ItemThumbnail";
import type { Item } from "../../types";

interface ItemTableProps {
  items: Item[];
  isReadOnly: boolean;
  currency: string;
  onEdit: (item: Item) => void;
  onDelete: (item: Item) => void;
}

export function ItemTable({ items, isReadOnly, currency, onEdit, onDelete }: ItemTableProps) {
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
          {items.map((item) => (
            <tr key={item.id} className="hover:bg-slate-50">
              <td className="px-4 py-3">
                <ItemThumbnail imagePath={item.imagePath} name={item.name} />
              </td>
              <td className="px-4 py-3 font-medium text-slate-900">{item.name}</td>
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
                  {item.stockQty}
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
          ))}
        </tbody>
      </table>
    </div>
  );
}
