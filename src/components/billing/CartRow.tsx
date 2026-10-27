import { Minus, NotebookPen, Plus, Trash2 } from "lucide-react";
import { useBillingStore } from "../../store";
import { formatMinor, formatQty } from "../../utils/format";
import { lineTotalMinor } from "../../utils/billingTotals";
import { ItemImage } from "./ItemImage";

interface CartRowProps {
  itemId: number;
  currency: string;
  onEditLine: (itemId: number) => void;
}

/**
 * Subscribes only to its own cart entry (`state.cart[itemId]`), not the whole
 * cart array — changing one line's quantity re-renders this row and the
 * totals footer, nothing else in the list.
 */
export function CartRow({ itemId, currency, onEditLine }: CartRowProps) {
  const entry = useBillingStore((state) => state.cart[itemId]);
  const setQty = useBillingStore((state) => state.setQty);
  const removeItem = useBillingStore((state) => state.removeItem);
  const isActive = useBillingStore((state) => state.activeLineItemId === itemId);
  const setActiveLine = useBillingStore((state) => state.setActiveLine);

  if (!entry) return null;

  // Rounded to whole minor units, same as `computeCartTotals` and the
  // server's per-line rounding — an amount-entered line's qty is carried at
  // full precision, so the raw product is fractional (9999.999999999998 for
  // a typed 100.00) and must not be shown or summed as-is.
  const lineMinor = lineTotalMinor(entry.priceMinor, entry.qty);

  return (
    <li
      onClick={() => setActiveLine(itemId)}
      // The row's separator line (previously the parent `<ul>`'s `divide-y`)
      // is folded into this same `border` declaration rather than left as a
      // separate utility on the list — a `divide-y` border-top and this
      // row's own border-color compete for the same side, and `divide-y`'s
      // `> :not(:first-child)` selector wins on specificity, leaving the
      // active highlight's top edge a stray slate color instead of brand
      // (a broken-looking top-left corner). Keeping every side's color on
      // this one element, with the divider only added when inactive (an
      // active row's border is a uniform brand-300 rectangle, no divider
      // fighting it on the last real edge case: an active row that isn't
      // the last one in the cart), avoids that conflict entirely.
      //
      // No negative margin here: the 8px the highlight bleeds past the text
      // column comes from the parent `<ul>`'s `-mx-2 px-2` instead. That list
      // is a scroll container (`overflow-y-auto`), which clips horizontally
      // too — a row that reached into it with its own `-mx-2` had its left
      // padding, border and rounded corner cut off, leaving the thumbnail
      // flush against the clipped edge whenever the row was active.
      className={[
        "flex cursor-pointer items-start gap-2 rounded-xl border px-2.5 py-2 transition-colors",
        isActive
          ? "border-accent/40 bg-accent-soft"
          : "border-line bg-surface-2/40 hover:border-line-strong",
      ].join(" ")}
    >
      <ItemImage imagePath={entry.imagePath} alt={entry.name} className="h-9 w-9 shrink-0 rounded-lg" />

      {/* Two lines — name (with remove) on top, price/note/qty/total below —
          so a narrow checkout column never squeezes the name to one letter. */}
      <div className="min-w-0 flex-1 space-y-1">
        <div className="flex items-start gap-1.5">
          <p className="line-clamp-2 min-w-0 flex-1 wrap-break-word text-sm font-semibold leading-snug text-fg" title={entry.name}>
            {entry.name}
          </p>
          <button
            type="button"
            onClick={() => removeItem(itemId)}
            className="shrink-0 rounded-lg p-1 text-fg-faint hover:bg-danger-soft hover:text-danger-text"
            aria-label={`Remove ${entry.name} from cart`}
          >
            <Trash2 className="h-3.5 w-3.5" />
          </button>
        </div>

        <div className="flex items-center gap-1.5">
          {/* Unit price only where the column is wide enough; qty × total says it otherwise. */}
          <p className="hidden min-w-0 flex-1 truncate text-xs tabular-nums text-fg-muted 2xl:block">{formatMinor(entry.priceMinor, currency)}</p>
          <span className="flex-1 2xl:hidden" />

          <button
            type="button"
            onClick={() => onEditLine(itemId)}
            aria-label={entry.notes ? `Edit note for ${entry.name}` : `Add a note to ${entry.name}`}
            title={entry.notes || "Add a note"}
            className={[
              "flex h-7 w-7 shrink-0 items-center justify-center rounded-lg",
              entry.notes ? "bg-accent text-accent-ink" : "bg-surface-3 text-fg-muted hover:bg-surface-3",
            ].join(" ")}
          >
            <NotebookPen className="h-3 w-3" />
          </button>

          <div className="flex shrink-0 items-center gap-1">
            <button
              type="button"
              onClick={() =>
                // A soldByAmount line rarely sits on a whole number (e.g.
                // 0.77 kg) — subtracting a flat 1 would usually just hit the
                // store's own 0.01 floor uselessly, so decrement removes the
                // line outright below 1 instead, same as it already does for a
                // normal item at qty 1. Fine-tuning a fractional qty down
                // (without removing it) goes through the notes-pencil edit
                // modal, which has a real decimal input.
                entry.qty <= 1 ? removeItem(itemId) : setQty(itemId, entry.qty - 1)
              }
              className="flex h-7 w-7 items-center justify-center rounded-lg bg-surface-3 text-fg-muted hover:bg-surface-3 disabled:opacity-30"
              aria-label={`Decrease quantity of ${entry.name}`}
            >
              <Minus className="h-3 w-3" />
            </button>
            <span className="min-w-5 text-center text-sm font-semibold tabular-nums text-fg">
              {formatQty(entry.qty, entry.unit)}
            </span>
            <button
              type="button"
              onClick={() => setQty(itemId, entry.qty + 1)}
              disabled={entry.qty >= entry.stockQty}
              className="flex h-7 w-7 items-center justify-center rounded-lg bg-surface-3 text-fg-muted hover:bg-surface-3 disabled:opacity-30"
              aria-label={`Increase quantity of ${entry.name}`}
            >
              <Plus className="h-3 w-3" />
            </button>
          </div>

          <div className="shrink-0 text-right">
            <span className="whitespace-nowrap text-sm font-bold tabular-nums text-fg">
              {formatMinor(lineMinor, currency)}
            </span>
          </div>
        </div>
      </div>
    </li>
  );
}
