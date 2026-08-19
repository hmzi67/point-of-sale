import { useState } from "react";
import { Minus, Plus, X } from "lucide-react";
import { categoryColor } from "../../utils/categoryColor";
import { formatMinor } from "../../utils/format";
import { ItemImage } from "./ItemImage";
import type { Item } from "../../types";

interface ItemDetailModalProps {
  item: Item;
  currency: string;
  /** Pre-fills the stepper/notes when reopened from a cart row's edit
   * pencil, so editing a line starts from what's already there. */
  initialQty?: number;
  initialNotes?: string;
  confirmLabel?: string;
  onClose: () => void;
  onConfirm: (qty: number, notes: string) => void;
}

/** Tapping an item card opens this instead of adding straight to the cart —
 * the one real interaction change from the previous screen. The same modal
 * doubles as the cart row's "edit this line" flow (pre-filled, different
 * button label), so there's one qty/notes editor, not two. */
export function ItemDetailModal({
  item,
  currency,
  initialQty = 1,
  initialNotes = "",
  confirmLabel = "Add to Cart",
  onClose,
  onConfirm,
}: ItemDetailModalProps) {
  const [qty, setQty] = useState(Math.min(Math.max(initialQty, 1), Math.max(item.stockQty, 1)));
  const [notes, setNotes] = useState(initialNotes);

  const clampQty = (next: number) => Math.max(1, Math.min(next, Math.max(item.stockQty, 1)));
  const color = categoryColor(item.categoryId);
  const lineTotalMinor = item.priceMinor * qty;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/40 p-4">
      <div className="w-full max-w-sm overflow-hidden rounded-3xl bg-white shadow-soft-lg">
        <div className="flex items-center justify-between border-b border-slate-100 px-5 py-3.5">
          <h3 className="text-sm font-semibold text-slate-900">Detail Menu</h3>
          <button type="button" onClick={onClose} className="rounded-full p-1 text-slate-400 hover:bg-slate-100 hover:text-slate-600">
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="px-5 pt-4">
          <ItemImage imagePath={item.imagePath} alt={item.name} className="h-40 w-full rounded-2xl" />
        </div>

        <div className="space-y-2 px-5 pt-4">
          {item.categoryName && (
            <span className={`inline-block rounded-full px-2.5 py-0.5 text-xs font-medium ${color.bg} ${color.text}`}>
              {item.categoryName}
            </span>
          )}
          <h2 className="text-lg font-bold text-slate-900">{item.name}</h2>
          {item.description && <p className="text-sm text-slate-500">{item.description}</p>}
          <p className="text-xl font-bold text-brand-700">{formatMinor(item.priceMinor, currency)}</p>

          <textarea
            value={notes}
            onChange={(e) => setNotes(e.target.value)}
            rows={2}
            placeholder="Add notes to your order…"
            className="w-full resize-none rounded-xl border border-slate-200 bg-slate-50 px-3.5 py-2.5 text-sm placeholder:text-slate-400 focus:border-brand-400 focus:outline-none"
          />

          <div className="flex items-center justify-center gap-5 py-1">
            <button
              type="button"
              onClick={() => setQty((q) => clampQty(q - 1))}
              disabled={qty <= 1}
              className="flex h-9 w-9 items-center justify-center rounded-full bg-slate-100 text-slate-600 hover:bg-slate-200 disabled:opacity-30"
              aria-label="Decrease quantity"
            >
              <Minus className="h-4 w-4" />
            </button>
            <span className="w-8 text-center text-base font-semibold text-slate-900">{qty}</span>
            <button
              type="button"
              onClick={() => setQty((q) => clampQty(q + 1))}
              disabled={qty >= item.stockQty}
              className="flex h-9 w-9 items-center justify-center rounded-full bg-slate-100 text-slate-600 hover:bg-slate-200 disabled:opacity-30"
              aria-label="Increase quantity"
            >
              <Plus className="h-4 w-4" />
            </button>
          </div>
          {qty >= item.stockQty && (
            <p className="text-center text-xs text-amber-600">Only {item.stockQty} in stock</p>
          )}
        </div>

        <div className="p-5 pt-3">
          <button
            type="button"
            onClick={() => onConfirm(qty, notes.trim())}
            className="w-full rounded-2xl bg-brand-600 py-3.5 text-sm font-semibold text-white shadow-soft hover:bg-brand-700"
          >
            {confirmLabel} ({formatMinor(lineTotalMinor, currency)})
          </button>
        </div>
      </div>
    </div>
  );
}
