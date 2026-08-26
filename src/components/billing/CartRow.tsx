import { Minus, NotebookPen, Plus, Trash2 } from "lucide-react";
import { useBillingStore } from "../../store";
import { formatMinor, formatQty } from "../../utils/format";
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

  if (!entry) return null;

  const lineTotalMinor = entry.priceMinor * entry.qty;

  return (
    <li className="flex items-center gap-3 py-3">
      <ItemImage imagePath={entry.imagePath} alt={entry.name} className="h-12 w-12 shrink-0 rounded-xl" />

      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-medium text-slate-900">{entry.name}</p>
        <p className="text-xs text-slate-500">{formatMinor(entry.priceMinor, currency)}</p>
      </div>

      <button
        type="button"
        onClick={() => onEditLine(itemId)}
        aria-label={entry.notes ? `Edit note for ${entry.name}` : `Add a note to ${entry.name}`}
        title={entry.notes || "Add a note"}
        className={[
          "flex h-6 w-6 shrink-0 items-center justify-center rounded-full",
          entry.notes ? "bg-brand-600 text-white" : "bg-slate-100 text-slate-400 hover:bg-slate-200",
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
          className="flex h-6 w-6 items-center justify-center rounded-full bg-slate-100 text-slate-500 hover:bg-slate-200 disabled:opacity-30"
          aria-label={`Decrease quantity of ${entry.name}`}
        >
          <Minus className="h-3 w-3" />
        </button>
        <span className="min-w-5 text-center text-sm font-medium text-slate-900">
          {formatQty(entry.qty, entry.unit)}
        </span>
        <button
          type="button"
          onClick={() => setQty(itemId, entry.qty + 1)}
          disabled={entry.qty >= entry.stockQty}
          className="flex h-6 w-6 items-center justify-center rounded-full bg-slate-100 text-slate-500 hover:bg-slate-200 disabled:opacity-30"
          aria-label={`Increase quantity of ${entry.name}`}
        >
          <Plus className="h-3 w-3" />
        </button>
      </div>

      <button
        type="button"
        onClick={() => removeItem(itemId)}
        className="shrink-0 rounded-full p-1 text-slate-300 hover:bg-red-50 hover:text-red-500"
        aria-label={`Remove ${entry.name} from cart`}
      >
        <Trash2 className="h-3.5 w-3.5" />
      </button>

      <span className="sr-only">{formatMinor(lineTotalMinor, currency)} line total</span>
    </li>
  );
}
