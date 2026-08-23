import { Flame, Minus, Plus } from "lucide-react";
import { categoryColor } from "../../utils/categoryColor";
import { formatMinor } from "../../utils/format";
import { useBillingStore } from "../../store";
import { ItemImage } from "./ItemImage";
import type { Item } from "../../types";

interface ItemCardProps {
  item: Item;
  currency: string;
  /** True when this item is in the current best-sellers ranking (see
   * `useBestSellerIds`) — shows a small fire badge on the thumbnail. */
  isBestSeller?: boolean;
}

/**
 * The card itself is no longer a button — there's nothing left to "open".
 * Cart interaction lives inline: an "Add to Cart" bar when the item isn't in
 * the cart yet, replaced by a qty stepper (subscribed to just this item's
 * cart entry, same pattern as `CartRow`) once it is. Decrementing from 1
 * removes the line entirely and reverts the card back to "Add to Cart".
 */
export function ItemCard({ item, currency, isBestSeller = false }: ItemCardProps) {
  const outOfStock = item.stockQty <= 0;
  const color = categoryColor(item.categoryId);

  const qty = useBillingStore((state) => state.cart[item.id]?.qty ?? 0);
  const addItem = useBillingStore((state) => state.addItem);
  const setQty = useBillingStore((state) => state.setQty);
  const removeItem = useBillingStore((state) => state.removeItem);

  const decrement = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (qty <= 1) removeItem(item.id);
    else setQty(item.id, qty - 1);
  };

  const increment = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (qty >= item.stockQty) return;
    setQty(item.id, qty + 1);
  };

  return (
    <div
      className={[
        "flex flex-col overflow-hidden rounded-2xl bg-white text-left shadow-soft ring-1 ring-slate-900/[0.03] transition-all duration-200 ease-out",
        outOfStock ? "opacity-50" : "hover:-translate-y-1 hover:shadow-soft-lg hover:ring-brand-200",
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
          {isBestSeller && !outOfStock && (
            <span
              className="absolute right-2 top-2 flex h-6 w-6 items-center justify-center rounded-full bg-orange-500/90 text-white shadow-sm"
              title="Best seller"
              aria-label="Best seller"
            >
              <Flame className="h-3.5 w-3.5" />
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

        {qty > 0 ? (
          <div className="mt-1 flex items-center justify-between rounded-xl bg-slate-100 p-1">
            <button
              type="button"
              onClick={decrement}
              className="flex h-7 w-7 items-center justify-center rounded-lg bg-white text-slate-600 shadow-sm hover:bg-slate-50"
              aria-label={`Decrease quantity of ${item.name}`}
            >
              <Minus className="h-3.5 w-3.5" />
            </button>
            <span className="text-sm font-semibold text-slate-900">{qty}</span>
            <button
              type="button"
              onClick={increment}
              disabled={qty >= item.stockQty}
              className="flex h-7 w-7 items-center justify-center rounded-lg bg-white text-slate-600 shadow-sm hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-40"
              aria-label={`Increase quantity of ${item.name}`}
            >
              <Plus className="h-3.5 w-3.5" />
            </button>
          </div>
        ) : (
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              if (!outOfStock) addItem(item);
            }}
            disabled={outOfStock}
            className="mt-1 rounded-xl bg-brand-600 py-1.5 text-xs font-semibold text-white hover:bg-brand-700 disabled:cursor-not-allowed disabled:bg-slate-200 disabled:text-slate-400"
          >
            Add to Cart
          </button>
        )}
      </div>
    </div>
  );
}
