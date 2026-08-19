import { Minus, Plus, Trash2 } from "lucide-react";
import { useBillingStore } from "../../store";
import { formatMinor } from "../../utils/format";

interface CartRowProps {
  itemId: number;
  currency: string;
}

/**
 * Subscribes only to its own cart entry (`state.cart[itemId]`), not the whole
 * cart array — changing one line's quantity re-renders this row and the
 * totals footer, nothing else in the list.
 */
export function CartRow({ itemId, currency }: CartRowProps) {
  const entry = useBillingStore((state) => state.cart[itemId]);
  const setQty = useBillingStore((state) => state.setQty);
  const removeItem = useBillingStore((state) => state.removeItem);

  if (!entry) return null;

  const lineTotalMinor = entry.priceMinor * entry.qty;

  return (
    <li className="flex items-center gap-3 px-4 py-3">
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-medium text-slate-900">{entry.name}</p>
        <p className="text-xs text-slate-500">{formatMinor(entry.priceMinor, currency)} each</p>
      </div>

      <div className="flex shrink-0 items-center gap-1">
        <button
          type="button"
          onClick={() => setQty(itemId, entry.qty - 1)}
          disabled={entry.qty <= 1}
          className="rounded-md p-1.5 text-slate-500 hover:bg-slate-100 disabled:opacity-30"
          aria-label={`Decrease quantity of ${entry.name}`}
        >
          <Minus className="h-3.5 w-3.5" />
        </button>
        <input
          type="number"
          min={1}
          max={entry.stockQty}
          value={entry.qty}
          onChange={(e) => setQty(itemId, Number(e.target.value) || 1)}
          className="w-12 rounded-md border border-slate-200 py-1 text-center text-sm"
        />
        <button
          type="button"
          onClick={() => setQty(itemId, entry.qty + 1)}
          disabled={entry.qty >= entry.stockQty}
          className="rounded-md p-1.5 text-slate-500 hover:bg-slate-100 disabled:opacity-30"
          aria-label={`Increase quantity of ${entry.name}`}
        >
          <Plus className="h-3.5 w-3.5" />
        </button>
      </div>

      <div className="w-20 shrink-0 text-right text-sm font-semibold text-slate-900">
        {formatMinor(lineTotalMinor, currency)}
      </div>

      <button
        type="button"
        onClick={() => removeItem(itemId)}
        className="shrink-0 rounded-md p-1.5 text-slate-400 hover:bg-red-50 hover:text-red-600"
        aria-label={`Remove ${entry.name} from cart`}
      >
        <Trash2 className="h-4 w-4" />
      </button>
    </li>
  );
}
