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
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-scrim p-4">
      <div className="w-full max-w-sm overflow-hidden rounded-3xl bg-surface shadow-soft-lg">
        <div className="flex items-center justify-between border-b border-line px-5 py-3.5">
          <h3 className="flex items-center gap-2 text-sm font-semibold text-fg">
            <Wallet className="h-4 w-4 text-accent-text" />
            Open Shift
          </h3>
          <button type="button" onClick={onClose} className="rounded-full p-1 text-fg-muted hover:bg-surface-3 hover:text-fg-2">
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="space-y-3 px-5 py-4">
          <p className="text-sm text-fg-muted">
            Count the cash in the drawer before your first sale, so the close-out reconciliation has a starting
            point.
          </p>
          <label className="block">
            <span className="mb-1 block text-xs font-medium text-fg-muted">Opening balance ({currency})</span>
            <input
              type="number"
              min={0}
              step="0.01"
              autoFocus
              value={amount}
              onChange={(e) => setAmount(e.target.value)}
              placeholder="0.00"
              className="w-full rounded-2xl border border-line bg-surface-2 px-3.5 py-2.5 text-sm focus:border-accent focus:outline-none"
            />
          </label>
          <p className="text-xs text-fg-muted">{formatMinor(openingBalanceMinor, currency)}</p>
          {error && <p className="text-sm text-danger-text">{error}</p>}
        </div>

        <div className="flex gap-2 p-5 pt-1">
          <button
            type="button"
            onClick={onClose}
            className="flex-1 rounded-2xl border border-line py-2.5 text-sm font-semibold text-fg-2 hover:bg-surface-2"
          >
            Skip
          </button>
          <button
            type="button"
            onClick={() => void submit()}
            disabled={isSubmitting}
            className="flex-1 rounded-2xl bg-accent py-2.5 text-sm font-semibold text-accent-ink shadow-soft hover:bg-accent-hover disabled:opacity-50"
          >
            {isSubmitting ? "Opening…" : "Open Shift"}
          </button>
        </div>
      </div>
    </div>
  );
}
