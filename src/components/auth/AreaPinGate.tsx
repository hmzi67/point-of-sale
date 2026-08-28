import { useEffect, useRef, useState, type FormEvent, type ReactNode } from "react";
import { Lock } from "lucide-react";
import { revokeAreaAccess, verifyAreaPin } from "../../services/securityPinService";

interface AreaPinGateProps {
  /** Shown in the prompt ("Enter the area PIN to open Salary"). */
  label: string;
  children: ReactNode;
}

/**
 * Wraps Attendance/Expenses/Salary/Employees/Reports — the shared
 * "sensitive area" PIN (`db::security_pin` on the Rust side) gates all
 * five, for every role, on every visit. `unlocked` lives in local component
 * state, not a store, deliberately: since each of those five is its own
 * `<Route>` in `App.tsx` (not nested tabs of one persistent layout), React
 * Router unmounts this component entirely on navigating away and mounts a
 * fresh instance on navigating back — so "every single visit needs its own
 * PIN" falls out naturally from React's own lifecycle, no explicit timeout
 * or session-tracking needed here.
 *
 * The unmount cleanup calls `revokeAreaAccess()` so the server-side grant
 * (`AreaAccessSession`) doesn't outlive actually being on the gated screen —
 * without it, a grant would otherwise just sit valid until its own
 * short safety-net timeout, which would let a stray direct `invoke()` call
 * from off-screen sneak through in the meantime.
 */
export function AreaPinGate({ label, children }: AreaPinGateProps) {
  const [unlocked, setUnlocked] = useState(false);
  const [pin, setPin] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [isVerifying, setIsVerifying] = useState(false);
  // Mirrors `unlocked` into a ref so the unmount cleanup below can read its
  // latest value without needing `unlocked` in the effect's own dependency
  // array (which would tear down and rebuild the cleanup on every unlock —
  // functionally harmless here, but this reads more directly as "the one
  // cleanup that runs at the very end").
  const wasUnlocked = useRef(false);

  // Revoke on unmount only if this visit actually got unlocked — an
  // unmount while still on the PIN prompt (never verified) has nothing to
  // revoke, and calling it anyway would be harmless but pointless.
  useEffect(() => {
    return () => {
      if (wasUnlocked.current) void revokeAreaAccess();
    };
  }, []);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (pin.length < 4) return;
    setIsVerifying(true);
    setError(null);
    try {
      await verifyAreaPin(pin);
      wasUnlocked.current = true;
      setUnlocked(true);
    } catch (err) {
      setError((err as Error).message);
      setPin("");
    } finally {
      setIsVerifying(false);
    }
  };

  if (unlocked) return <>{children}</>;

  return (
    <div className="mx-auto flex max-w-sm flex-col items-center gap-4 rounded-lg border border-slate-200 bg-white p-8 text-center shadow-sm">
      <span className="flex h-12 w-12 items-center justify-center rounded-full bg-brand-50 text-brand-600">
        <Lock className="h-5 w-5" />
      </span>
      <div>
        <h2 className="text-base font-semibold text-slate-900">Enter the area PIN</h2>
        <p className="mt-1 text-sm text-slate-500">Required to open {label}.</p>
      </div>

      <form onSubmit={(e) => void submit(e)} className="w-full space-y-3">
        <input
          type="password"
          inputMode="numeric"
          autoFocus
          maxLength={6}
          value={pin}
          onChange={(e) => setPin(e.target.value.replace(/\D/g, ""))}
          placeholder="PIN"
          className="w-full rounded-md border border-slate-300 px-3 py-2 text-center text-lg tracking-[0.5em] focus:border-brand-400 focus:outline-none"
        />

        {error && <p className="text-sm text-red-600">{error}</p>}

        <button
          type="submit"
          disabled={pin.length < 4 || isVerifying}
          className="w-full rounded-md bg-brand-600 px-3 py-2 text-sm font-semibold text-white hover:bg-brand-700 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {isVerifying ? "Checking…" : "Unlock"}
        </button>
      </form>
    </div>
  );
}
