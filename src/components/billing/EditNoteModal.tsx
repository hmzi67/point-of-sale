import { useState } from "react";
import { X } from "lucide-react";
import { useAppConfig } from "../../hooks/useAppConfig";
import { decimalToMinor, formatMinor, formatQty, roundQty } from "../../utils/format";
import { ModalOverlay } from "../ui/ModalOverlay";

interface EditNoteModalProps {
  itemName: string;
  initialNotes: string;
  onClose: () => void;
  /** `qty` is only ever passed back when `amountEntry` was given — same
   * "commit on Save, not before" timing the note field already has, so
   * there's no race between a live qty write and this callback re-sending
   * a stale one. Always the final absolute line quantity (already merged
   * with the existing qty when the cashier used the "Amount" tab below) —
   * `onSave` never needs to know which tab produced it. */
  onSave: (notes: string, qty?: number) => void;
  /** Only present for a `soldByAmount` line — its current qty, unit
   * ("kg"), per-unit price and remaining stock. When set, this modal also
   * shows a qty/amount editor alongside the note; a normal per-piece item
   * already has the cart row's ±1 stepper for that, so this stays
   * note-only for it, unchanged from before this feature. */
  amountEntry?: { qty: number; unit: string | null; priceMinor: number; stockQty: number };
}

/**
 * The cart row's pencil icon opens this — primarily a note editor. Quantity
 * for a normal item is already adjustable straight from the cart row (and,
 * on the grid, straight from the item card) via a whole-number ±1 stepper,
 * so there's no qty control here for one; a `soldByAmount` line is the one
 * exception, since its qty is a fractional real-world weight that the ±1
 * stepper isn't precise enough to fine-tune.
 *
 * `amountEntry` being set adds two tabs for that case:
 *  - "Quantity" — the original decimal qty field, an absolute correction
 *    ("the scale actually read 0.81 kg") that overwrites the line's qty.
 *  - "Amount" — mirrors `ItemAmountEntryModal`'s amount-to-qty math (qty =
 *    amount ÷ item.price), but for a line already in the cart. Matches
 *    `addItemByAmount`'s merge behavior: it *adds* the computed qty to
 *    whatever is already on the line (e.g. 2 units already there + "₹100
 *    more" combines into one line at the new total qty), it never
 *    overwrites — a customer buying 2 pieces by count and then asking for
 *    ₹100 loose still gets one line, one correct total, not a silent
 *    replace of the 2 they already had.
 */
export function EditNoteModal({ itemName, initialNotes, onClose, onSave, amountEntry }: EditNoteModalProps) {
  const { config } = useAppConfig();
  const [notes, setNotes] = useState(initialNotes);
  const [qtyText, setQtyText] = useState(amountEntry ? String(amountEntry.qty) : "");
  const [amountText, setAmountText] = useState("");
  const [mode, setMode] = useState<"qty" | "amount">("qty");

  const amountMinor = (() => {
    const amount = Number(amountText);
    return Number.isFinite(amount) && amount > 0 ? decimalToMinor(amount) : 0;
  })();

  const addedQty =
    amountEntry && amountMinor > 0 ? roundQty(amountMinor / amountEntry.priceMinor) : 0;

  const combinedQty = amountEntry
    ? Math.min(amountEntry.qty + addedQty, amountEntry.stockQty)
    : 0;

  const save = () => {
    if (!amountEntry) {
      onSave(notes.trim());
      return;
    }

    if (mode === "amount") {
      if (addedQty <= 0) return; // nothing entered — leave the line as-is
      onSave(notes.trim(), combinedQty);
      return;
    }

    const parsed = Number(qtyText);
    const qty = Number.isFinite(parsed) && parsed > 0 ? parsed : amountEntry.qty; // invalid edit -> keep as-is
    onSave(notes.trim(), qty);
  };

  return (
    <ModalOverlay>
      <div className="w-full max-w-sm overflow-hidden rounded-3xl bg-surface shadow-soft-lg">
        <div className="flex items-center justify-between border-b border-line px-5 py-3.5">
          <h3 className="text-sm font-semibold text-fg">
            {amountEntry ? `${itemName} — qty & note` : `Note for ${itemName}`}
          </h3>
          <button type="button" onClick={onClose} className="rounded-full p-1 text-fg-muted hover:bg-surface-3 hover:text-fg-2">
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="space-y-3 px-5 pt-4">
          {amountEntry && (
            <div>
              <div className="mb-2 grid grid-cols-2 gap-1.5 rounded-xl bg-surface-3 p-1">
                <button
                  type="button"
                  onClick={() => setMode("qty")}
                  className={[
                    "rounded-lg py-1.5 text-xs font-semibold transition-colors",
                    mode === "qty" ? "bg-surface text-accent-text shadow-sm" : "text-fg-muted hover:text-fg-2",
                  ].join(" ")}
                >
                  Quantity
                </button>
                <button
                  type="button"
                  onClick={() => setMode("amount")}
                  className={[
                    "rounded-lg py-1.5 text-xs font-semibold transition-colors",
                    mode === "amount" ? "bg-surface text-accent-text shadow-sm" : "text-fg-muted hover:text-fg-2",
                  ].join(" ")}
                >
                  Amount
                </button>
              </div>

              {mode === "qty" ? (
                <label className="block">
                  <span className="mb-1 block text-xs font-medium text-fg-muted">
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
                    className="w-32 rounded-xl border border-line bg-surface-2 px-3.5 py-2.5 text-sm focus:border-accent focus:outline-none"
                  />
                  <span className="mt-1 block text-[11px] text-fg-muted">
                    E.g. after weighing on a scale — this replaces the line's quantity.
                  </span>
                </label>
              ) : (
                <label className="block">
                  <span className="mb-1 block text-xs font-medium text-fg-muted">Add amount ({config.currency})</span>
                  <input
                    type="number"
                    min="0"
                    step="0.01"
                    inputMode="decimal"
                    value={amountText}
                    onChange={(e) => setAmountText(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        e.preventDefault();
                        save();
                      }
                    }}
                    autoFocus
                    placeholder="e.g. 100"
                    className="w-32 rounded-xl border border-line bg-surface-2 px-3.5 py-2.5 text-sm focus:border-accent focus:outline-none"
                  />
                  <div className="mt-2 rounded-xl bg-accent-soft px-3.5 py-2.5 text-xs text-accent-text">
                    {amountMinor > 0 ? (
                      <>
                        + {formatQty(addedQty, amountEntry.unit)} added to the {formatQty(amountEntry.qty, amountEntry.unit)}{" "}
                        already on this line — new total{" "}
                        <span className="font-semibold">{formatQty(combinedQty, amountEntry.unit)}</span>
                        {amountEntry.qty + addedQty > amountEntry.stockQty && (
                          <span className="mt-1 block text-danger-text">Capped by available stock.</span>
                        )}
                      </>
                    ) : (
                      <span className="text-accent">
                        Type an amount to add to the existing {formatQty(amountEntry.qty, amountEntry.unit)} on this
                        line.
                      </span>
                    )}
                  </div>
                  <span className="mt-1 block text-[11px] text-fg-muted">
                    E.g. "customer also wants {amountEntry.priceMinor > 0 ? formatMinor(amountEntry.priceMinor, config.currency) : ""}{" "}
                    worth more" — this adds to the line, it doesn't replace it.
                  </span>
                </label>
              )}
            </div>
          )}

          <textarea
            value={notes}
            onChange={(e) => setNotes(e.target.value)}
            rows={3}
            autoFocus={!amountEntry}
            placeholder="Add notes to your order…"
            className="w-full resize-none rounded-xl border border-line bg-surface-2 px-3.5 py-2.5 text-sm placeholder:text-fg-muted focus:border-accent focus:outline-none"
          />
        </div>

        <div className="flex gap-2 p-5 pt-4">
          <button
            type="button"
            onClick={onClose}
            className="flex-1 rounded-2xl bg-surface-3 py-3 text-sm font-semibold text-fg-2 hover:bg-surface-3"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={save}
            disabled={amountEntry !== undefined && mode === "amount" && addedQty <= 0}
            className="flex-1 rounded-2xl bg-accent py-3 text-sm font-semibold text-accent-ink shadow-soft hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-50"
          >
            {amountEntry && mode === "amount" ? "Add to Line" : "Save"}
          </button>
        </div>
      </div>
    </ModalOverlay>
  );
}
