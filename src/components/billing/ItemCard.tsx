import { categoryColor } from "../../utils/categoryColor";
import { formatMinor } from "../../utils/format";
import { ItemImage } from "./ItemImage";
import type { Item } from "../../types";

interface ItemCardProps {
  item: Item;
  currency: string;
  onOpen: (item: Item) => void;
}

/** Tapping a card opens the item detail modal — it never adds to the cart
 * directly, unlike the search bar's barcode/Enter path. Out-of-stock items
 * stay visible (so a cashier can see what's out) but can't be opened. */
export function ItemCard({ item, currency, onOpen }: ItemCardProps) {
  const outOfStock = item.stockQty <= 0;
  const color = categoryColor(item.categoryId);

  return (
    <button
      type="button"
      onClick={() => onOpen(item)}
      disabled={outOfStock}
      className={[
        "flex flex-col overflow-hidden rounded-2xl bg-white text-left shadow-soft transition-transform",
        outOfStock ? "cursor-not-allowed opacity-50" : "hover:-translate-y-0.5 hover:shadow-soft-lg",
      ].join(" ")}
    >
      <ItemImage imagePath={item.imagePath} alt={item.name} className="h-28 w-full" />
      <div className="flex flex-1 flex-col gap-1 p-3">
        <p className="truncate text-sm font-semibold text-slate-900">{item.name}</p>
        <div className="flex items-center justify-between gap-2">
          {item.categoryName ? (
            <span className={`truncate rounded-full px-2 py-0.5 text-[11px] font-medium ${color.bg} ${color.text}`}>
              {item.categoryName}
            </span>
          ) : (
            <span />
          )}
          <span className="shrink-0 text-sm font-bold text-slate-900">{formatMinor(item.priceMinor, currency)}</span>
        </div>
        {outOfStock && <span className="text-[11px] font-medium text-red-500">Out of stock</span>}
      </div>
    </button>
  );
}
