import { useState, type FormEvent } from "react";
import { adjustStock } from "../../services/stockService";
import type { AdjustmentKind, Item } from "../../types";
import { formatQty } from "../../utils/format";
import { runAction } from "../../utils/runAction";
import { Field, inputClass, ModalCard, primaryButtonClass, secondaryButtonClass } from "../ui/ModalCard";

const KINDS: { value: AdjustmentKind; label: string; hint: string }[] = [
  { value: "add", label: "Add stock", hint: "Stock received outside a purchase order" },
  { value: "damage", label: "Damaged / lost", hint: "Written off — expired, broken, stolen" },
  { value: "correction", label: "Set count", hint: "Replace the on-hand figure with what's on the shelf" },
];

/** Adds inventory, writes off damage, or corrects one item's count — each
 * logged as a stock movement. */
export function StockAdjustModal({ item, onSaved, onClose }: { item: Item; onSaved: () => void; onClose: () => void }) {
  const [kind, setKind] = useState<AdjustmentKind>("add");
  const [qty, setQty] = useState("");
  const [note, setNote] = useState("");
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    await runAction(setIsSaving, setError, async () => {
      await adjustStock({ itemId: item.id, kind, qty: Number(qty), note: note.trim() || null });
      onSaved();
    });
  };

  return (
    <ModalCard title={`Adjust stock — ${item.name}`} subtitle={`On hand: ${formatQty(item.stockQty, item.unit)}`} width="sm" onClose={onClose}>
      <form onSubmit={(e) => void submit(e)} className="space-y-3">
        <div className="space-y-1.5">
          {KINDS.map((k) => (
            <label key={k.value} className={`flex cursor-pointer items-start gap-3 rounded-xl border p-3 ${kind === k.value ? "border-accent/40 bg-accent-soft" : "border-line"}`}>
              <input type="radio" name="kind" checked={kind === k.value} onChange={() => setKind(k.value)} className="mt-0.5" />
              <span>
                <span className="block text-sm font-semibold text-fg">{k.label}</span>
                <span className="block text-xs text-fg-muted">{k.hint}</span>
              </span>
            </label>
          ))}
        </div>
        <Field label={kind === "correction" ? "Counted quantity" : "Quantity"}>
          <input type="number" min="0" step="any" value={qty} onChange={(e) => setQty(e.target.value)} autoFocus className={inputClass} />
        </Field>
        <Field label="Reason / note">
          <input value={note} onChange={(e) => setNote(e.target.value)} placeholder={kind === "damage" ? "e.g. expired, broken in transit" : "Optional"} className={inputClass} />
        </Field>
        {error && <p className="text-xs text-danger-text">{error}</p>}
        <div className="flex justify-end gap-2 pt-1">
          <button type="button" onClick={onClose} className={secondaryButtonClass}>Cancel</button>
          <button type="submit" disabled={isSaving || qty.trim() === "" || Number(qty) < 0} className={primaryButtonClass}>
            {isSaving ? "Saving…" : "Save"}
          </button>
        </div>
      </form>
    </ModalCard>
  );
}
