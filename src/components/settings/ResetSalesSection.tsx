import { useState, type FormEvent } from "react";
import { AlertTriangle, Trash2 } from "lucide-react";
import { resetSales } from "../../services/securityPinService";
import { useAuthStore, useBillingStore } from "../../store";
import { runAction } from "../../utils/runAction";
import { ModalOverlay } from "../ui/ModalOverlay";

const CONFIRM_WORD = "DELETE";

/** Owner-only "danger zone": wipes the whole sales history after a warning
 * dialog where the Owner has to type DELETE (re-checked in Rust by
 * `security_reset_sales`). */
export function ResetSalesSection() {
  const isOwner = useAuthStore((state) => state.user?.role === "owner");
  const [open, setOpen] = useState(false);
  const [done, setDone] = useState(false);

  if (!isOwner) return null;

  return (
    <div className="rounded-2xl border border-danger-line bg-surface p-6">
      <h2 className="flex items-center gap-2 text-base font-semibold tracking-tight text-danger-text">
        <Trash2 className="h-4 w-4" />
        Reset sales data
      </h2>
      <p className="mt-1 text-sm text-fg-2">
        Permanently deletes every sale, refund, held/active order, kitchen token and pay-later installment. Items, stock,
        customers, staff and settings are kept.
      </p>
      {done && <p className="mt-3 rounded-xl bg-success-soft px-3 py-2 text-sm text-success-text">All sales data was deleted.</p>}
      <button
        type="button"
        onClick={() => {
          setDone(false);
          setOpen(true);
        }}
        className="mt-4 rounded-xl bg-danger px-4 py-2.5 text-sm font-semibold text-white hover:opacity-90"
      >
        Reset all sales…
      </button>
      {open && (
        <ResetSalesDialog
          onClose={() => setOpen(false)}
          onDone={() => {
            setOpen(false);
            setDone(true);
          }}
        />
      )}
    </div>
  );
}

function ResetSalesDialog({ onClose, onDone }: { onClose: () => void; onDone: () => void }) {
  const [typed, setTyped] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const matches = typed === CONFIRM_WORD;

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!matches) return;
    const ok = await runAction(setBusy, setError, () => resetSales(typed));
    if (ok) {
      useBillingStore.getState().clearCart(); // a cart may point at a deleted order
      onDone();
    }
  };

  return (
    <ModalOverlay>
      <form
        onSubmit={(e) => void submit(e)}
        role="dialog"
        aria-modal="true"
        aria-labelledby="reset-sales-title"
        className="w-full max-w-md space-y-4 rounded-3xl bg-surface p-6 shadow-soft-lg"
      >
        <div className="flex items-start gap-3">
          <AlertTriangle className="mt-0.5 h-5 w-5 shrink-0 text-danger-text" aria-hidden />
          <div>
            <h2 id="reset-sales-title" className="text-base font-semibold text-fg">
              Delete all sales data?
            </h2>
            <p className="mt-1 text-sm text-fg-muted">
              This permanently deletes every sale, refund, order, kitchen token and pay-later installment. Reports and
              the dashboard will start from zero. <strong className="text-danger-text">This cannot be undone.</strong>
            </p>
            <p className="mt-2 text-sm text-fg-muted">
              If this counter is connected to a business, the sales are deleted on every counter of that business.
            </p>
          </div>
        </div>
        <label className="block text-sm text-fg-2">
          Type <span className="font-mono font-bold text-fg">{CONFIRM_WORD}</span> to confirm
          <input
            autoFocus
            value={typed}
            onChange={(e) => setTyped(e.target.value)}
            autoComplete="off"
            spellCheck={false}
            className="mt-1 w-full rounded-xl border border-line-strong bg-surface px-3 py-2.5 font-mono text-sm text-fg focus:border-danger focus:outline-none"
          />
        </label>
        {error && <p className="text-sm text-danger-text">{error}</p>}
        <div className="flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            disabled={busy}
            className="rounded-xl border border-line px-4 py-2 text-sm font-medium text-fg-2 hover:bg-surface-2 disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            type="submit"
            disabled={!matches || busy}
            className="rounded-xl bg-danger px-4 py-2 text-sm font-semibold text-white hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-40"
          >
            {busy ? "Deleting…" : "Delete all sales"}
          </button>
        </div>
      </form>
    </ModalOverlay>
  );
}
