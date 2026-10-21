import { Flame, PackageSearch, Pencil, Trash2 } from "lucide-react";
import { categoryColor } from "../../utils/categoryColor";
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
      <div className="flex flex-col items-center gap-3 rounded-2xl border border-dashed border-line-strong bg-surface px-5 py-16 text-center text-sm text-fg-muted">
        <span className="rounded-2xl bg-accent-soft p-4 text-accent-text"><PackageSearch className="h-7 w-7" strokeWidth={1.5} /></span>
        <p className="font-semibold text-fg">No products to show</p>
        <p className="max-w-xs text-xs leading-relaxed">Try changing your search or filters. If your catalog is empty, add your first item to get started.</p>
      </div>
    );
  }

  return (
    <div className="overflow-x-auto rounded-2xl border border-line bg-surface">
      <table className="w-full min-w-[860px] text-left text-sm">
        <thead className="border-b border-line bg-surface-2/60 text-xs font-semibold text-fg-2">
          <tr>
            {canSelect && (
              <th className="w-10 px-4 py-3.5" aria-hidden="false">
                <input
                  type="checkbox"
                  checked={allSelected}
                  ref={(el) => {
                    if (el) el.indeterminate = someSelected;
                  }}
                  onChange={onToggleSelectAll}
                  aria-label="Select all items"
                  className="h-4 w-4 rounded border-line-strong accent-accent focus:ring-accent"
                />
              </th>
            )}
            <th className="px-4 py-3.5 font-semibold">Product</th>
            <th className="px-4 py-3.5 font-semibold">Barcode</th>
            <th className="px-4 py-3.5 font-semibold">Category</th>
            <th className="px-4 py-3.5 text-right font-semibold">Selling price</th>
            <th className="px-4 py-3.5 text-right font-semibold">Cost</th>
            <th className="px-4 py-3.5 text-right font-semibold">Stock</th>
            <th className="px-4 py-3.5 font-semibold">Availability</th>
            {!isReadOnly && <th className="px-4 py-3.5 text-right font-semibold">Actions</th>}
          </tr>
        </thead>
        <tbody className="divide-y divide-line">
          {items.map((item) => {
            const color = categoryColor(item.categoryId);
            const isSelected = !!selectedIds?.has(item.id);
            return (
              <tr key={item.id} className={`transition-colors ${isSelected ? "bg-accent-soft" : "hover:bg-surface-2/60"}`}>
                {canSelect && (
                  <td className="px-4 py-3.5">
                    <input
                      type="checkbox"
                      checked={isSelected}
                      onChange={() => onToggleSelect!(item.id)}
                      aria-label={`Select ${item.name}`}
                      className="h-4 w-4 rounded border-line-strong accent-accent focus:ring-accent"
                    />
                  </td>
                )}
                <td className="px-4 py-3.5 font-medium text-fg">
                  <div className="flex items-center gap-3">
                    <ItemThumbnail imagePath={item.imagePath} name={item.name} size={44} />
                    <span className="inline-flex max-w-72 items-center gap-2 leading-relaxed">
                    {item.name}
                    {bestSellerIds?.has(item.id) && (
                      <span title="Best seller">
                        <Flame className="h-3.5 w-3.5 shrink-0 text-warning-text" aria-label="Best seller" />
                      </span>
                    )}
                    </span>
                  </div>
                </td>
                <td className="px-4 py-3.5 font-mono text-xs text-fg-muted">{item.barcode ?? "—"}</td>
                <td className="px-4 py-3.5 text-fg-2"><span className={`inline-block rounded-lg px-2.5 py-1 text-xs font-semibold ${color.bg} ${color.text}`}>{item.categoryName ?? "Uncategorized"}</span></td>
                <td className="whitespace-nowrap px-4 py-3.5 text-right font-bold text-fg tabular-nums">{formatMinor(item.priceMinor, currency)}</td>
                <td className="whitespace-nowrap px-4 py-3.5 text-right text-fg-muted tabular-nums">{formatMinor(item.costMinor, currency)}</td>
                <td className="whitespace-nowrap px-4 py-3.5 text-right font-semibold text-fg tabular-nums">
                  {formatQty(item.stockQty, item.unit)}
                </td>
                <td className="px-4 py-3.5">
                  <span className={`inline-flex items-center gap-1.5 whitespace-nowrap rounded-lg px-2.5 py-1 text-[11px] font-medium ${item.stockQty <= 0 ? "bg-danger-soft text-danger-text" : item.isLowStock ? "bg-warning-soft text-warning-text" : "bg-success-soft text-success-text"}`}>
                    <span className="h-1.5 w-1.5 rounded-full bg-current" />
                    {item.stockQty <= 0 ? "Out of stock" : item.isLowStock ? "Low stock" : "In stock"}
                  </span>
                </td>
                {!isReadOnly && (
                  <td className="px-4 py-3">
                    <div className="flex justify-end gap-2">
                      <button
                        type="button"
                        onClick={() => onEdit(item)}
                        className="rounded-lg border border-line p-2 text-fg-muted hover:bg-surface-3 hover:text-fg"
                        aria-label={`Edit ${item.name}`}
                      >
                        <Pencil className="h-4 w-4" />
                      </button>
                      <button
                        type="button"
                        onClick={() => onDelete(item)}
                        className="rounded-lg border border-line p-2 text-fg-muted hover:bg-danger-soft hover:text-danger-text"
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
