import { useState, type FormEvent } from "react";
import { PackagePlus } from "lucide-react";
import { addOutsideItem } from "../../services/billingService";
import { useBillingStore } from "../../store";
import { decimalToMinor } from "../../utils/format";
import { runAction } from "../../utils/runAction";
import { Field, ModalCard, inputClass, primaryButtonClass, secondaryButtonClass } from "../ui/ModalCard";

/** Adds something not on the menu (brought or bought from outside) to the
 * bill as a name and a price. It becomes an ordinary cart line, so both
 * receipts print it like any other item. */
export function OutsideItemModal({ currency, onClose }: { currency: string; onClose: () => void }) {
  const addItem = useBillingStore((state) => state.addItem);
  const setActiveLine = useBillingStore((state) => state.setActiveLine);
  const [name, setName] = useState("");
  const [price, setPrice] = useState("");
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const priceValue = Number(price);
  const isValid = name.trim() !== "" && price.trim() !== "" && Number.isFinite(priceValue) && priceValue >= 0;

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!isValid || isSaving) return;
    await runAction(setIsSaving, setError, async () => {
      const item = await addOutsideItem(name.trim(), decimalToMinor(priceValue));
      addItem(item);
      setActiveLine(item.id);
      onClose();
    });
  };

  return (
    <ModalCard title="Outside item" subtitle="Add something that isn't on the menu" icon={PackagePlus} width="sm" onClose={onClose}>
      <form onSubmit={submit} className="space-y-4">
        {error && <p className="rounded-xl bg-danger-soft p-3 text-xs text-danger-text">{error}</p>}
        <Field label="Name">
          <input autoFocus value={name} onChange={(e) => setName(e.target.value)} maxLength={80} className={inputClass} placeholder="e.g. Birthday cake" />
        </Field>
        <Field label={`Price (${currency})`}>
          <input
            type="number"
            inputMode="decimal"
            min="0"
            step="any"
            value={price}
            onChange={(e) => setPrice(e.target.value)}
            className={inputClass}
            placeholder="0"
          />
        </Field>
        <div className="flex justify-end gap-2">
          <button type="button" onClick={onClose} className={secondaryButtonClass}>Cancel</button>
          <button type="submit" disabled={!isValid || isSaving} className={primaryButtonClass}>
            {isSaving ? "Adding…" : "Add to bill"}
          </button>
        </div>
      </form>
    </ModalCard>
  );
}
