import { useState } from "react";
import { Wallet, X } from "lucide-react";
import { useShiftStore } from "../../store";
import { formatMinor } from "../../utils/format";

interface OpenShiftModalProps {
  currency: string;
  onClose: () => void;
  onOpened: () => void;
}

/** Prompts a cashier for their opening cash float before they can start
 * ringing up sales against a shift. Skippable entirely (via the close
 * button) — a client that enabled the `shifts` module but doesn't want to
 * force this every login still sees the "Open Shift" affordance sit there,
 * not a blocking wall. */
export function OpenShiftModal({ currency, onClose, onOpened }: OpenShiftModalProps) {
  const [amount, setAmount] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const open = useShiftStore((state) => state.open);

  const openingBalanceMinor = Math.round((Number(amount) || 0) * 100);

  const submit = async () => {
    setIsSubmitting(true);
    setError(null);
    try {
      await open(openingBalanceMinor);
      onOpened();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/40 p-4">
      <div className="w-full max-w-sm overflow-hidden rounded-3xl bg-white shadow-soft-lg">
        <div className="flex items-center justify-between border-b border-slate-100 px-5 py-3.5">
          <h3 className="flex items-center gap-2 text-sm font-semibold text-slate-900">
            <Wallet className="h-4 w-4 text-brand-600" />
            Open Shift
          </h3>
          <button type="button" onClick={onClose} className="rounded-full p-1 text-slate-400 hover:bg-slate-100 hover:text-slate-600">
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="space-y-3 px-5 py-4">
          <p className="text-sm text-slate-500">
            Count the cash in the drawer before your first sale, so the close-out reconciliation has a starting
            point.
          </p>
          <label className="block">
            <span className="mb-1 block text-xs font-medium text-slate-500">Opening balance ({currency})</span>
            <input
              type="number"
              min={0}
              step="0.01"
              autoFocus
              value={amount}
              onChange={(e) => setAmount(e.target.value)}
              placeholder="0.00"
              className="w-full rounded-2xl border border-slate-200 bg-slate-50 px-3.5 py-2.5 text-sm focus:border-brand-400 focus:outline-none"
            />
          </label>
          <p className="text-xs text-slate-400">{formatMinor(openingBalanceMinor, currency)}</p>
          {error && <p className="text-sm text-red-600">{error}</p>}
        </div>

        <div className="flex gap-2 p-5 pt-1">
          <button
            type="button"
            onClick={onClose}
            className="flex-1 rounded-2xl border border-slate-200 py-2.5 text-sm font-semibold text-slate-600 hover:bg-slate-50"
          >
            Skip
          </button>
          <button
            type="button"
            onClick={() => void submit()}
            disabled={isSubmitting}
            className="flex-1 rounded-2xl bg-brand-600 py-2.5 text-sm font-semibold text-white shadow-soft hover:bg-brand-700 disabled:opacity-50"
          >
            {isSubmitting ? "Opening…" : "Open Shift"}
          </button>
        </div>
      </div>
    </div>
  );
}
