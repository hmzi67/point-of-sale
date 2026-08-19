import { useState, type FormEvent } from "react";
import { X } from "lucide-react";
import { recordPayment } from "../../services/salaryService";
import { decimalToMinor, formatMinor } from "../../utils/format";
import type { SalaryCalculation } from "../../types";

function todayString(): string {
  const now = new Date();
  return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(now.getDate()).padStart(2, "0")}`;
}

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
  const [amount, setAmount] = useState(remaining > 0 ? (remaining / 100).toFixed(2) : "");
  const [date, setDate] = useState(todayString());
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const amountMinor = decimalToMinor(Number(amount));
    if (!amountMinor || amountMinor <= 0 || !date) return;

    setIsSaving(true);
    setError(null);
    try {
      const updated = await recordPayment(row.employeeId, row.month, amountMinor, date);
      onSaved(updated);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/40 p-4">
      <div className="w-full max-w-sm rounded-lg bg-white shadow-xl">
        <div className="flex items-center justify-between border-b border-slate-200 px-5 py-4">
          <div>
            <h3 className="text-sm font-semibold text-slate-900">Record payment</h3>
            <p className="text-xs text-slate-500">
              {row.employeeName} · {row.month}
            </p>
          </div>
          <button type="button" onClick={onClose} className="text-slate-400 hover:text-slate-600">
            <X className="h-4 w-4" />
          </button>
        </div>

        <form onSubmit={(e) => void submit(e)} className="space-y-3 px-5 py-4">
          <dl className="grid grid-cols-2 gap-2 rounded-md bg-slate-50 px-3 py-2 text-xs text-slate-600">
            <div>
              <dt>Calculated</dt>
              <dd className="font-medium text-slate-900">{formatMinor(row.calculatedAmountMinor, currency)}</dd>
            </div>
            <div>
              <dt>Already paid</dt>
              <dd className="font-medium text-slate-900">{formatMinor(row.paidAmountMinor, currency)}</dd>
            </div>
          </dl>

          <div>
            <label className="block text-xs font-medium text-slate-500">Amount</label>
            <input
              type="number"
              min={0}
              step="0.01"
              autoFocus
              value={amount}
              onChange={(e) => setAmount(e.target.value)}
              className="mt-1 w-full rounded-md border border-slate-300 px-2.5 py-1.5 text-sm"
            />
          </div>

          <div>
            <label className="block text-xs font-medium text-slate-500">Date</label>
            <input
              type="date"
              value={date}
              max={todayString()}
              onChange={(e) => setDate(e.target.value)}
              className="mt-1 w-full rounded-md border border-slate-300 px-2.5 py-1.5 text-sm"
            />
          </div>

          {error && <p className="text-xs text-red-600">{error}</p>}

          <div className="flex justify-end gap-2 pt-1">
            <button
              type="button"
              onClick={onClose}
              className="rounded-md border border-slate-300 px-3 py-1.5 text-sm font-medium text-slate-600 hover:bg-slate-50"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={isSaving || !amount}
              className="rounded-md bg-brand-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-brand-700 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {isSaving ? "Saving…" : "Save payment"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
