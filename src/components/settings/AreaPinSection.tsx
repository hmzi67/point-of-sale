import { useState, type FormEvent } from "react";
import { KeyRound } from "lucide-react";
import { setAreaPin } from "../../services/securityPinService";
import { runAction } from "../../utils/runAction";

/**
 * Changes the shared "sensitive area" PIN (`AreaPinGate`/`db::security_pin`)
 * that gates Attendance, Expenses, Salary, Employees and Reports for every
 * role. Settings itself is already Owner/Admin-only (see `App.tsx`'s route
 * guard), and `security_set_area_pin` re-checks that server-side too — this
 * form has no separate role check of its own to duplicate.
 */
export function AreaPinSection() {
  const [oldPin, setOldPin] = useState("");
  const [newPin, setNewPin] = useState("");
  const [confirmPin, setConfirmPin] = useState("");
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState(false);

  const mismatch = confirmPin.length > 0 && newPin !== confirmPin;
  const canSubmit = oldPin.length >= 4 && newPin.length >= 4 && newPin === confirmPin;

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!canSubmit) return;
    setSuccess(false);
    await runAction(setIsSaving, setError, async () => {
      await setAreaPin(oldPin, newPin);
      setOldPin("");
      setNewPin("");
      setConfirmPin("");
      setSuccess(true);
    });
  };

  return (
    <div className="rounded-2xl border border-line bg-surface p-6">
      <h2 className="flex items-center gap-2 text-base font-semibold tracking-tight text-fg">
        <KeyRound className="h-4 w-4 text-fg-muted" />
        Area PIN
      </h2>
      <p className="mt-1 text-sm text-fg-2">
        Required to open Attendance, Expenses, Salary, Employees and Reports — for every account,
        every visit, regardless of role.
      </p>

      <form onSubmit={(e) => void submit(e)} className="mt-4 grid max-w-sm gap-3">
        <label className="block">
          <span className="mb-1 block text-xs font-medium text-fg-muted">Current PIN</span>
          <input
            type="password"
            inputMode="numeric"
            maxLength={6}
            value={oldPin}
            onChange={(e) => {
              setOldPin(e.target.value.replace(/\D/g, ""));
              setSuccess(false);
            }}
            className="w-full rounded-xl border border-line-strong px-3 py-2.5 text-sm focus:border-accent focus:outline-none"
          />
        </label>

        <label className="block">
          <span className="mb-1 block text-xs font-medium text-fg-muted">New PIN</span>
          <input
            type="password"
            inputMode="numeric"
            maxLength={6}
            value={newPin}
            onChange={(e) => {
              setNewPin(e.target.value.replace(/\D/g, ""));
              setSuccess(false);
            }}
            placeholder="4-6 digits"
            className="w-full rounded-xl border border-line-strong px-3 py-2.5 text-sm focus:border-accent focus:outline-none"
          />
        </label>

        <label className="block">
          <span className="mb-1 block text-xs font-medium text-fg-muted">Confirm new PIN</span>
          <input
            type="password"
            inputMode="numeric"
            maxLength={6}
            value={confirmPin}
            onChange={(e) => {
              setConfirmPin(e.target.value.replace(/\D/g, ""));
              setSuccess(false);
            }}
            className="w-full rounded-xl border border-line-strong px-3 py-2.5 text-sm focus:border-accent focus:outline-none"
          />
          {mismatch && <span className="mt-1 block text-xs text-danger-text">PINs don't match.</span>}
        </label>

        {error && <p className="text-sm text-danger-text">{error}</p>}
        {success && <p className="text-sm text-success-text">Area PIN updated.</p>}

        <button
          type="submit"
          disabled={!canSubmit || isSaving}
          className="mt-1 w-fit rounded-xl bg-accent px-3 py-2.5 text-sm font-medium text-accent-ink hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-50"
        >
          {isSaving ? "Saving…" : "Change PIN"}
        </button>
      </form>
    </div>
  );
}
