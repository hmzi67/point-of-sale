import { useState } from "react";
import { X } from "lucide-react";

interface EditNoteModalProps {
  itemName: string;
  initialNotes: string;
  onClose: () => void;
  onSave: (notes: string) => void;
}

/**
 * The cart row's pencil icon opens this — a focused note editor, nothing
 * else. Quantity is already adjustable straight from the cart row (and, on
 * the grid, straight from the item card), so there's no qty control here;
 * this replaces the note field the old add-time ItemDetailModal used to
 * carry, now that add-to-cart itself happens inline on the card.
 */
export function EditNoteModal({ itemName, initialNotes, onClose, onSave }: EditNoteModalProps) {
  const [notes, setNotes] = useState(initialNotes);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/40 p-4">
      <div className="w-full max-w-sm overflow-hidden rounded-3xl bg-white shadow-soft-lg">
        <div className="flex items-center justify-between border-b border-slate-100 px-5 py-3.5">
          <h3 className="text-sm font-semibold text-slate-900">Note for {itemName}</h3>
          <button type="button" onClick={onClose} className="rounded-full p-1 text-slate-400 hover:bg-slate-100 hover:text-slate-600">
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="px-5 pt-4">
          <textarea
            value={notes}
            onChange={(e) => setNotes(e.target.value)}
            rows={3}
            autoFocus
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
            onClick={() => onSave(notes.trim())}
            className="flex-1 rounded-2xl bg-brand-600 py-3 text-sm font-semibold text-white shadow-soft hover:bg-brand-700"
          >
            Save Note
          </button>
        </div>
      </div>
    </div>
  );
}
