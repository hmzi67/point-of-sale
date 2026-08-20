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
        "flex flex-col overflow-hidden rounded-2xl bg-white text-left shadow-soft ring-1 ring-slate-900/[0.03] transition-all duration-200 ease-out",
        outOfStock ? "cursor-not-allowed opacity-50" : "hover:-translate-y-1 hover:shadow-soft-lg hover:ring-brand-200",
      ].join(" ")}
    >
      {/* Padded, tinted mount for the photo instead of an edge-to-edge crop —
       * reads as a framed thumbnail floating on a soft neutral surface,
       * inset within the card, not filling its whole top edge. */}
      <div className="p-2.5 pb-0">
        <div className="relative overflow-hidden rounded-xl bg-slate-50">
          <ItemImage imagePath={item.imagePath} alt={item.name} className="h-28 w-full" />
          {outOfStock && (
            <span className="absolute left-2 top-2 rounded-full bg-red-500/90 px-2 py-0.5 text-[10px] font-semibold text-white">
              Out of stock
            </span>
          )}
        </div>
      </div>

      {/* Name is the anchor; category pill + price share one row below it,
       * consistent padding all round so image → name → row reads as one
       * block instead of separately-spaced pieces. */}
      <div className="flex flex-1 flex-col gap-1.5 p-3 pt-2.5">
        <p className="truncate text-sm font-semibold text-slate-900">{item.name}</p>
        <div className="flex items-center justify-between gap-2">
          {item.categoryName ? (
            <span className={`truncate rounded-full px-2 py-0.5 text-[10.5px] font-medium ${color.bg} ${color.text}`}>
              {item.categoryName}
            </span>
          ) : (
            <span />
          )}
          <span className="shrink-0 text-base font-extrabold tracking-tight text-slate-900">
            {formatMinor(item.priceMinor, currency)}
          </span>
        </div>
      </div>
    </button>
  );
}
