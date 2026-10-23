import { useState, type FormEvent } from "react";
import { recordSupplierPayment } from "../../services/purchasingService";
import type { Supplier, SupplierPaymentMethod } from "../../types";
import { paymentMethodLabel, SUPPLIER_PAYMENT_METHODS } from "../../utils/checkoutPayments";
import { todayString } from "../../utils/dateRanges";
import { decimalToMinor, formatMinor, minorToDecimal } from "../../utils/format";
import { runAction } from "../../utils/runAction";
import { Field, inputClass, ModalCard, primaryButtonClass, secondaryButtonClass } from "../ui/ModalCard";

export function SupplierPaymentModal({
  supplier,
  currency,
  onSaved,
  onClose,
}: {
  supplier: Supplier;
  currency: string;
  onSaved: () => void;
  onClose: () => void;
}) {
  const [amount, setAmount] = useState("");
  const [method, setMethod] = useState<SupplierPaymentMethod>("cash");
  const [reference, setReference] = useState("");
  const [note, setNote] = useState("");
  const [paidDate, setPaidDate] = useState(todayString());
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    await runAction(setIsSaving, setError, async () => {
      await recordSupplierPayment({
        supplierId: supplier.id,
        amountMinor: decimalToMinor(Number(amount)),
        method,
        reference: reference.trim() || null,
        note: note.trim() || null,
        paidDate,
      });
      onSaved();
    });
  };

  return (
    <ModalCard title={`Pay ${supplier.name}`} subtitle={`You owe ${formatMinor(supplier.balanceMinor, currency)}`} width="sm" onClose={onClose}>
      <form onSubmit={(e) => void submit(e)} className="space-y-3">
        <Field label={`Amount (${currency})`}>
          <div className="flex gap-2">
            <input type="number" min="0" step="0.01" value={amount} onChange={(e) => setAmount(e.target.value)} autoFocus className={inputClass} />
            {supplier.balanceMinor > 0 && (
              <button type="button" onClick={() => setAmount(String(minorToDecimal(supplier.balanceMinor)))} className={secondaryButtonClass}>
                All
              </button>
            )}
          </div>
        </Field>
        <Field label="Method">
          <div className="grid grid-cols-4 gap-2">
            {SUPPLIER_PAYMENT_METHODS.map((m) => (
              <button
                key={m}
                type="button"
                aria-pressed={method === m}
                onClick={() => setMethod(m)}
                className={`rounded-xl border py-2 text-xs font-semibold ${method === m ? "border-accent/40 bg-accent-soft text-accent-text" : "border-line text-fg-muted"}`}
              >
                {paymentMethodLabel(m)}
              </button>
            ))}
          </div>
        </Field>
        <div className="grid grid-cols-2 gap-3">
          <Field label="Date">
            <input type="date" value={paidDate} onChange={(e) => setPaidDate(e.target.value)} className={inputClass} />
          </Field>
          <Field label="Reference">
            <input value={reference} onChange={(e) => setReference(e.target.value)} placeholder="Cheque / txn no." className={inputClass} />
          </Field>
        </div>
        <Field label="Note">
          <input value={note} onChange={(e) => setNote(e.target.value)} placeholder="Optional" className={inputClass} />
        </Field>
        {error && <p className="text-xs text-danger-text">{error}</p>}
        <div className="flex justify-end gap-2 pt-1">
          <button type="button" onClick={onClose} className={secondaryButtonClass}>Cancel</button>
          <button type="submit" disabled={isSaving || !(Number(amount) > 0)} className={primaryButtonClass}>
            {isSaving ? "Saving…" : "Record payment"}
          </button>
        </div>
      </form>
    </ModalCard>
  );
}
