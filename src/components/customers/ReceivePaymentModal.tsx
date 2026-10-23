import { useEffect, useState, type FormEvent } from "react";
import { getCreditSales, recordCustomerPayment } from "../../services/customersService";
import type { CollectionMethod, CreditSale, Customer } from "../../types";
import { COLLECTION_METHODS, paymentMethodLabel } from "../../utils/checkoutPayments";
import { decimalToMinor, formatMinor, minorToDecimal } from "../../utils/format";
import { runAction } from "../../utils/runAction";
import { Field, inputClass, ModalCard, primaryButtonClass, secondaryButtonClass } from "../ui/ModalCard";

/** Records an installment against one bill, or on account (oldest due
 * first). Cash taken here counts in the receiver's open shift. */
export function ReceivePaymentModal({
  customer,
  saleId = null,
  currency,
  onSaved,
  onClose,
}: {
  customer: Customer;
  saleId?: number | null;
  currency: string;
  onSaved: () => void;
  onClose: () => void;
}) {
  const [bills, setBills] = useState<CreditSale[]>([]);
  const [billId, setBillId] = useState<number | null>(saleId);
  const [amount, setAmount] = useState("");
  const [method, setMethod] = useState<CollectionMethod>("cash");
  const [note, setNote] = useState("");
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getCreditSales(customer.id, true)
      .then(setBills)
      .catch((e: Error) => setError(e.message));
  }, [customer.id]);

  const owed = billId === null ? customer.balanceMinor : bills.find((b) => b.saleId === billId)?.outstandingMinor ?? 0;

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    await runAction(setIsSaving, setError, async () => {
      await recordCustomerPayment({
        customerId: customer.id,
        saleId: billId,
        amountMinor: decimalToMinor(Number(amount)),
        method,
        note: note.trim() || null,
      });
      onSaved();
    });
  };

  return (
    <ModalCard title={`Receive payment — ${customer.name}`} subtitle={`Owes ${formatMinor(customer.balanceMinor, currency)}`} width="sm" onClose={onClose}>
      <form onSubmit={(e) => void submit(e)} className="space-y-3">
        <Field label="Against">
          <select value={billId ?? ""} onChange={(e) => setBillId(e.target.value ? Number(e.target.value) : null)} className={`${inputClass} pr-8`}>
            <option value="">On account (oldest due first)</option>
            {bills.map((bill) => (
              <option key={bill.saleId} value={bill.saleId}>
                Sale #{bill.saleId} — {formatMinor(bill.outstandingMinor, currency)} due {bill.dueDate ?? "—"}
              </option>
            ))}
          </select>
        </Field>
        <Field label={`Amount (${currency})`}>
          <div className="flex gap-2">
            <input type="number" min="0" step="0.01" value={amount} onChange={(e) => setAmount(e.target.value)} autoFocus className={inputClass} />
            <button type="button" onClick={() => setAmount(String(minorToDecimal(owed)))} className={secondaryButtonClass}>
              All
            </button>
          </div>
        </Field>
        <Field label="Method">
          <div className="grid grid-cols-3 gap-2">
            {COLLECTION_METHODS.map((m) => (
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
