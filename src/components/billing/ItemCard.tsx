import { Flame, Minus, Plus } from "lucide-react";
import { categoryIcon } from "../../utils/categoryIcon";
import { categoryColor } from "../../utils/categoryColor";
import { formatMinor, formatQty } from "../../utils/format";
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
 *
 * A `soldByAmount` item uses the exact same single button — it lands in the
 * cart at qty 0 (see `billingStore.addItem`), and the cashier sets its real
 * quantity by selecting that line in the cart panel and pressing Enter to
 * type an amount (see `useFastBillingHotkeys`/`ItemAmountEntryModal`), or by
 * using the stepper here/there like any other line.
 */
export function ItemCard({ item, currency, isBestSeller = false }: ItemCardProps) {
  const outOfStock = item.stockQty <= 0;
  const color = categoryColor(item.categoryId);
  const CategoryIcon = categoryIcon(item.categoryName ?? "");

  const inCart = useBillingStore((state) => state.cart[item.id] !== undefined);
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
        "group flex min-w-0 flex-col overflow-hidden rounded-2xl border bg-surface text-left transition-colors duration-200",
        inCart ? "border-accent/60 shadow-soft" : "border-line",
        outOfStock ? "opacity-50" : "hover:border-accent/40 hover:shadow-soft",
      ].join(" ")}
    >
      {/* Padded, tinted mount for the photo instead of an edge-to-edge crop —
       * reads as a framed thumbnail floating on a soft neutral surface,
       * inset within the card, not filling its whole top edge. */}
      <div className="p-2 sm:p-2.5 pb-0">
        <div className="relative overflow-hidden rounded-xl bg-surface-2">
          <ItemImage imagePath={item.imagePath} alt={item.name} className="h-24 sm:h-28 w-full"
            fallback={<span aria-hidden="true" className={`flex h-14 w-14 sm:h-16 sm:w-16 items-center justify-center rounded-2xl border border-current/10 ${color.bg} ${color.text}`}><CategoryIcon className="h-7 w-7 sm:h-8 sm:w-8" strokeWidth={1.25} /></span>} />
          <div className="absolute left-2 top-2 flex flex-col items-start gap-1">
            {item.shortCode && (
              <span
                className="rounded-full bg-overlay px-2 py-0.5 text-[10px] font-semibold tabular-nums text-overlay-ink"
                title={`Quick code ${item.shortCode}`}
              >
                #{item.shortCode}
              </span>
            )}
            {outOfStock && (
              <span className="rounded-full bg-danger/90 px-2 py-0.5 text-[10px] font-semibold text-white">
                Out of stock
              </span>
            )}
          </div>
          {isBestSeller && !outOfStock && (
            <span
              className="absolute right-2 top-2 flex h-6 w-6 items-center justify-center rounded-full bg-warning/90 text-white shadow-sm"
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
      <div className="flex flex-1 flex-col gap-1.5 p-2.5 sm:gap-2 sm:p-3">
        <p title={item.name} className="line-clamp-2 min-h-9 text-xs sm:text-sm font-semibold leading-snug text-fg">{item.name}</p>
        <div className="flex flex-wrap items-center justify-between gap-1.5">
          {item.categoryName ? (
            <span className={`truncate rounded-full px-2 py-0.5 text-[10px] sm:text-[10.5px] font-medium ${color.bg} ${color.text}`}>
              {item.categoryName}
            </span>
          ) : (
            <span />
          )}
          <span className="shrink-0 text-sm sm:text-base font-extrabold tracking-tight text-fg">
            {formatMinor(item.priceMinor, currency)}
          </span>
        </div>

        {inCart ? (
          <div className="mt-auto flex items-center justify-between rounded-xl border border-accent/20 bg-accent-soft p-1">
            <button
              type="button"
              onClick={decrement}
              className="flex h-7 w-7 sm:h-8 sm:w-8 items-center justify-center rounded-lg bg-surface text-fg-2 shadow-sm hover:bg-surface-2"
              aria-label={`Decrease quantity of ${item.name}`}
            >
              <Minus className="h-3.5 w-3.5" />
            </button>
            <span className="text-xs sm:text-sm font-semibold text-fg">{formatQty(qty, item.unit)}</span>
            <button
              type="button"
              onClick={increment}
              disabled={qty >= item.stockQty}
              className="flex h-7 w-7 sm:h-8 sm:w-8 items-center justify-center rounded-lg bg-surface text-fg-2 shadow-sm hover:bg-surface-2 disabled:cursor-not-allowed disabled:opacity-40"
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
            className="mt-auto flex min-h-9 items-center justify-center gap-1.5 rounded-xl border border-accent/20 bg-accent-soft py-1.5 sm:py-2 text-xs font-semibold text-accent-text transition-colors hover:bg-accent hover:text-accent-ink disabled:cursor-not-allowed disabled:bg-surface-3 disabled:text-fg-muted"
          >
            <Plus className="h-3.5 w-3.5" /> Add to cart
          </button>
        )}
      </div>
    </div>
  );
}
