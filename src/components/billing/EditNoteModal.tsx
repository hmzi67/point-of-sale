import { useState } from "react";
import { X } from "lucide-react";

interface EditNoteModalProps {
  itemName: string;
  initialNotes: string;
  onClose: () => void;
  /** `qty` is only ever passed back when `amountEntry` was given — same
   * "commit on Save, not before" timing the note field already has, so
   * there's no race between a live qty write and this callback re-sending
   * a stale one. */
  onSave: (notes: string, qty?: number) => void;
  /** Only present for a `soldByAmount` line — its current qty and unit
   * ("kg"). When set, this modal also shows a decimal qty field ("adjust
   * after weighing on a scale") alongside the note; a normal per-piece
   * item already has the cart row's ±1 stepper for that, so this stays
   * note-only for it, unchanged from before this feature. */
  amountEntry?: { qty: number; unit: string | null };
}

/**
 * The cart row's pencil icon opens this — primarily a note editor. Quantity
 * for a normal item is already adjustable straight from the cart row (and,
 * on the grid, straight from the item card) via a whole-number ±1 stepper,
 * so there's no qty control here for one; a `soldByAmount` line is the one
 * exception, since its qty is a fractional real-world weight that the ±1
 * stepper isn't precise enough to fine-tune (the actual amount handed over
 * after weighing on a scale rarely lands on a whole unit) — `amountEntry`
 * being set adds a decimal qty field for exactly that case.
 */
export function EditNoteModal({ itemName, initialNotes, onClose, onSave, amountEntry }: EditNoteModalProps) {
  const [notes, setNotes] = useState(initialNotes);
  const [qtyText, setQtyText] = useState(amountEntry ? String(amountEntry.qty) : "");

  const save = () => {
    if (!amountEntry) {
      onSave(notes.trim());
      return;
    }
    const parsed = Number(qtyText);
    const qty = Number.isFinite(parsed) && parsed > 0 ? parsed : amountEntry.qty; // invalid edit -> keep as-is
    onSave(notes.trim(), qty);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/40 p-4">
      <div className="w-full max-w-sm overflow-hidden rounded-3xl bg-white shadow-soft-lg">
        <div className="flex items-center justify-between border-b border-slate-100 px-5 py-3.5">
          <h3 className="text-sm font-semibold text-slate-900">
            {amountEntry ? `${itemName} — qty & note` : `Note for ${itemName}`}
          </h3>
          <button type="button" onClick={onClose} className="rounded-full p-1 text-slate-400 hover:bg-slate-100 hover:text-slate-600">
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="space-y-3 px-5 pt-4">
          {amountEntry && (
            <label className="block">
              <span className="mb-1 block text-xs font-medium text-slate-500">
                Quantity{amountEntry.unit ? ` (${amountEntry.unit})` : ""}
              </span>
              <input
                type="number"
                min="0.01"
                step="0.01"
                inputMode="decimal"
                value={qtyText}
                onChange={(e) => setQtyText(e.target.value)}
                autoFocus
                className="w-32 rounded-xl border border-slate-200 bg-slate-50 px-3.5 py-2.5 text-sm focus:border-brand-400 focus:outline-none"
              />
              <span className="mt-1 block text-[11px] text-slate-400">
                E.g. after weighing on a scale — this updates the line total.
              </span>
            </label>
          )}

          <textarea
            value={notes}
            onChange={(e) => setNotes(e.target.value)}
            rows={3}
            autoFocus={!amountEntry}
            placeholder="Add notes to your order…"
            className="w-full resize-none rounded-xl border border-slate-200 bg-slate-50 px-3.5 py-2.5 text-sm placeholder:text-slate-400 focus:border-brand-400 focus:outline-none"
          />
        </div>

        <div className="flex gap-2 p-5 pt-4">
          <button
            type="button"
            onClick={onClose}
            className="flex-1 rounded-2xl bg-slate-100 py-3 text-sm font-semibold text-slate-600 hover:bg-slate-200"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={save}
            className="flex-1 rounded-2xl bg-brand-600 py-3 text-sm font-semibold text-white shadow-soft hover:bg-brand-700"
          >
            Save
          </button>
        </div>
      </div>
    </div>
  );
}
