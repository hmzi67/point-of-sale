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
      <div className="rounded-lg border border-dashed border-line-strong bg-surface py-16 text-center text-sm text-fg-muted">
        No items match your search.
      </div>
    );
  }

  return (
    <div className="overflow-x-auto rounded-lg border border-line bg-surface">
      <table className="w-full min-w-[720px] text-left text-sm">
        <thead className="border-b border-line bg-surface-2 text-xs uppercase tracking-wide text-fg-muted">
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
                  className="h-4 w-4 rounded border-line-strong text-accent-text focus:ring-accent"
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
        <tbody className="divide-y divide-line">
          {items.map((item) => {
            const isSelected = !!selectedIds?.has(item.id);
            return (
              <tr key={item.id} className={isSelected ? "bg-accent-soft" : "hover:bg-surface-2"}>
                {canSelect && (
                  <td className="px-4 py-3">
                    <input
                      type="checkbox"
                      checked={isSelected}
                      onChange={() => onToggleSelect!(item.id)}
                      aria-label={`Select ${item.name}`}
                      className="h-4 w-4 rounded border-line-strong text-accent-text focus:ring-accent"
                    />
                  </td>
                )}
                <td className="px-4 py-3">
                  <ItemThumbnail imagePath={item.imagePath} name={item.name} />
                </td>
                <td className="px-4 py-3 font-medium text-fg">
                  <span className="inline-flex items-center gap-1.5">
                    {item.name}
                    {bestSellerIds?.has(item.id) && (
                      <span title="Best seller">
                        <Flame className="h-3.5 w-3.5 shrink-0 text-warning-text" aria-label="Best seller" />
                      </span>
                    )}
                  </span>
                </td>
                <td className="px-4 py-3 font-mono text-xs text-fg-muted">{item.barcode ?? "—"}</td>
                <td className="px-4 py-3 text-fg-2">{item.categoryName ?? "Uncategorized"}</td>
                <td className="px-4 py-3 text-right text-fg">{formatMinor(item.priceMinor, currency)}</td>
                <td className="px-4 py-3 text-right text-fg-muted">{formatMinor(item.costMinor, currency)}</td>
                <td className="px-4 py-3 text-right">
                  <span
                    className={[
                      "inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 font-medium",
                      item.isLowStock ? "bg-danger-soft text-danger-text" : "text-fg-2",
                    ].join(" ")}
                  >
                    {item.isLowStock && <span className="h-1.5 w-1.5 rounded-full bg-danger" />}
                    {formatQty(item.stockQty, item.unit)}
                  </span>
                </td>
                {!isReadOnly && (
                  <td className="px-4 py-3">
                    <div className="flex justify-end gap-1">
                      <button
                        type="button"
                        onClick={() => onEdit(item)}
                        className="rounded-md p-1.5 text-fg-muted hover:bg-surface-3 hover:text-fg"
                        aria-label={`Edit ${item.name}`}
                      >
                        <Pencil className="h-4 w-4" />
                      </button>
                      <button
                        type="button"
                        onClick={() => onDelete(item)}
                        className="rounded-md p-1.5 text-fg-muted hover:bg-danger-soft hover:text-danger-text"
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
