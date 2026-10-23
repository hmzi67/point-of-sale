import { useState, type FormEvent } from "react";
import { X } from "lucide-react";
import { recordPayment } from "../../services/salaryService";
import { decimalToMinor, formatMinor, minorToDecimal } from "../../utils/format";
import type { SalaryCalculation } from "../../types";
import { todayString } from "../../utils/dateRanges";
import { ModalOverlay } from "../ui/ModalOverlay";
import { runAction } from "../../utils/runAction";
import { inputClass } from "../ui/ModalCard";

interface RecordPaymentModalProps {
  row: SalaryCalculation;
  currency: string;
  onSaved: (updated: SalaryCalculation) => void;
  onClose: () => void;
}

/** A payment against one employee's month — amount and date. Adds to
 * whatever's already been paid that month rather than replacing it, so
 * partial instalments accumulate correctly (see `db::salary::record_payment`). */
export function RecordPaymentModal({ row, currency, onSaved, onClose }: RecordPaymentModalProps) {
  const remaining = Math.max(row.calculatedAmountMinor - row.paidAmountMinor, 0);
  const [amount, setAmount] = useState(remaining > 0 ? minorToDecimal(remaining).toFixed(2) : "");
  const [date, setDate] = useState(todayString());
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const amountMinor = decimalToMinor(Number(amount));
    if (!amountMinor || amountMinor <= 0 || !date) return;

    await runAction(setIsSaving, setError, async () => {
      const updated = await recordPayment(row.employeeId, row.month, amountMinor, date);
      onSaved(updated);
    });
  };

  return (
    <ModalOverlay>
      <div className="w-full max-w-sm rounded-2xl border border-line bg-surface shadow-xl">
        <div className="flex items-center justify-between border-b border-line px-5 py-4">
          <div>
            <h3 className="text-sm font-semibold text-fg">Record payment</h3>
            <p className="text-xs text-fg-muted">
              {row.employeeName} · {row.month}
            </p>
          </div>
          <button type="button" onClick={onClose} className="text-fg-muted hover:text-fg-2">
            <X className="h-4 w-4" />
          </button>
        </div>

        <form onSubmit={(e) => void submit(e)} className="space-y-3 px-5 py-4">
          <dl className="grid grid-cols-2 gap-2 rounded-xl bg-surface-2 px-3 py-2 text-xs text-fg-2">
            <div>
              <dt>Calculated</dt>
              <dd className="font-medium text-fg">{formatMinor(row.calculatedAmountMinor, currency)}</dd>
            </div>
            <div>
              <dt>Already paid</dt>
              <dd className="font-medium text-fg">{formatMinor(row.paidAmountMinor, currency)}</dd>
            </div>
          </dl>

          <div>
            <label className="block text-xs font-medium text-fg-muted">Amount</label>
            <input
              type="number"
              min={0}
              step="0.01"
              autoFocus
              value={amount}
              onChange={(e) => setAmount(e.target.value)}
              className={`mt-1 ${inputClass}`}
            />
          </div>

          <div>
            <label className="block text-xs font-medium text-fg-muted">Date</label>
            <input
              type="date"
              value={date}
              max={todayString()}
              onChange={(e) => setDate(e.target.value)}
              className={`mt-1 ${inputClass}`}
            />
          </div>

          {error && <p className="text-xs text-danger-text">{error}</p>}

          <div className="flex justify-end gap-2 pt-1">
            <button
              type="button"
              onClick={onClose}
              className="rounded-xl border border-line-strong px-3 py-1.5 text-sm font-medium text-fg-2 hover:bg-surface-2"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={isSaving || !amount}
              className="rounded-xl bg-accent px-3 py-1.5 text-sm font-medium text-accent-ink hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-50"
            >
              {isSaving ? "Saving…" : "Save payment"}
            </button>
          </div>
        </form>
      </div>
    </ModalOverlay>
  );
}
