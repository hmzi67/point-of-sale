import { ShieldAlert } from "lucide-react";
import { useEffect, useState, type FormEvent } from "react";
import { useLocation } from "react-router-dom";
import { getDefaultCredentials, replaceDefaultPins } from "../../services/securityPinService";
import { useAuthStore } from "../../store";
import type { DefaultCredentials } from "../../types";
import { isAdminRole } from "../../utils/permissions";
import { runAction } from "../../utils/runAction";
import { ModalOverlay } from "../ui/ModalOverlay";

/**
 * While the Owner login PIN or the shared area PIN is still the value every
 * install ships with, Owner/Admin can't use the app until they replace it:
 * a blocking dialog asks for whichever PINs they're allowed to change. Rust
 * decides what is default (`security_get_default_credentials` checks the
 * stored hashes) and only replaces a PIN that still is. An Admin can't change
 * the Owner's PIN (outranked), so for them a default Owner PIN stays a
 * banner. Re-checked on every navigation.
 */
export function DefaultPinBanner() {
  const role = useAuthStore((s) => s.user?.role);
  const location = useLocation();
  const [defaults, setDefaults] = useState<DefaultCredentials | null>(null);
  const isAdmin = role !== undefined && isAdminRole(role);

  useEffect(() => {
    if (!isAdmin) {
      setDefaults(null);
      return;
    }
    let cancelled = false;
    getDefaultCredentials()
      .then((result) => !cancelled && setDefaults(result))
      .catch(() => !cancelled && setDefaults(null));
    return () => {
      cancelled = true;
    };
  }, [isAdmin, location.pathname]);

  if (!defaults || (!defaults.ownerPinIsDefault && !defaults.areaPinIsDefault)) return null;

  const mustChangeOwner = defaults.ownerPinIsDefault && role === "owner";
  if (mustChangeOwner || defaults.areaPinIsDefault) {
    return (
      <ChangeDefaultPinsDialog
        owner={mustChangeOwner}
        area={defaults.areaPinIsDefault}
        onDone={() => setDefaults({ ...defaults, ownerPinIsDefault: !mustChangeOwner && defaults.ownerPinIsDefault, areaPinIsDefault: false })}
      />
    );
  }

  return (
    <div
      role="alert"
      className="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-danger-line bg-danger-soft px-4 py-2 text-sm text-danger-text"
    >
      <ShieldAlert className="h-4 w-4 shrink-0" aria-hidden />
      <span className="min-w-0 flex-1">
        <span className="font-semibold">Default PIN in use.</span> The Owner login PIN is still the default — ask the
        Owner to sign in and change it.
      </span>
    </div>
  );
}

const PIN_INPUT =
  "w-full rounded-md border border-line-strong bg-surface px-3 py-2 text-center text-lg tracking-[0.5em] focus:border-accent focus:outline-none";

function ChangeDefaultPinsDialog({ owner, area, onDone }: { owner: boolean; area: boolean; onDone: () => void }) {
  const [ownerPin, setOwnerPin] = useState("");
  const [ownerConfirm, setOwnerConfirm] = useState("");
  const [areaPin, setAreaPin] = useState("");
  const [areaConfirm, setAreaConfirm] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if ((owner && ownerPin !== ownerConfirm) || (area && areaPin !== areaConfirm)) {
      setError("The two PINs you typed don't match.");
      return;
    }
    const ok = await runAction(setBusy, setError, () =>
      replaceDefaultPins(owner ? ownerPin : null, area ? areaPin : null),
    );
    if (ok) onDone();
  };

  const pinField = (label: string, value: string, onChange: (v: string) => void) => (
    <label className="block text-left text-sm text-fg-2">
      {label}
      <input
        type="password"
        inputMode="numeric"
        autoComplete="new-password"
        maxLength={6}
        value={value}
        onChange={(e) => onChange(e.target.value.replace(/\D/g, ""))}
        className={`mt-1 ${PIN_INPUT}`}
      />
    </label>
  );

  return (
    <ModalOverlay>
      <form
        onSubmit={(e) => void submit(e)}
        role="dialog"
        aria-modal="true"
        aria-labelledby="default-pin-title"
        className="w-full max-w-sm space-y-4 rounded-3xl bg-surface p-6 shadow-soft-lg"
      >
        <div className="flex items-start gap-3">
          <ShieldAlert className="mt-0.5 h-5 w-5 shrink-0 text-danger-text" aria-hidden />
          <div>
            <h2 id="default-pin-title" className="text-base font-semibold text-fg">
              Change the default PIN{owner && area ? "s" : ""}
            </h2>
            <p className="mt-1 text-sm text-fg-muted">
              Every install ships with the same PINs, so anyone who knows them could sign in or open protected areas.
              Choose new 4–6 digit PINs to continue.
            </p>
          </div>
        </div>
        {owner && (
          <fieldset className="space-y-2">
            {pinField("New Owner login PIN", ownerPin, setOwnerPin)}
            {pinField("Confirm Owner login PIN", ownerConfirm, setOwnerConfirm)}
          </fieldset>
        )}
        {area && (
          <fieldset className="space-y-2">
            {pinField("New protected-area PIN", areaPin, setAreaPin)}
            {pinField("Confirm protected-area PIN", areaConfirm, setAreaConfirm)}
          </fieldset>
        )}
        {error && <p className="text-sm text-danger-text">{error}</p>}
        <button
          type="submit"
          disabled={busy}
          className="w-full rounded-md bg-accent px-3 py-2 text-sm font-semibold text-accent-ink hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-50"
        >
          {busy ? "Saving…" : "Save and continue"}
        </button>
      </form>
    </ModalOverlay>
  );
}
