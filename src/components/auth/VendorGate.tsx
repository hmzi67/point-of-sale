import { useEffect, useState, type FormEvent } from "react";
import { Outlet } from "react-router-dom";
import { ShieldCheck } from "lucide-react";
import { getVendorGateStatus, verifyVendorGate } from "../../services/vendorGateService";

/**
 * First-run vendor authorization gate, wrapped around the setup wizard.
 *
 * A fresh install has to clear this before setup can be completed. Once
 * `onboarding_completed` is true the backend reports `required: false` and
 * this renders its children unconditionally, forever — it is a setup-time
 * gate, not a recurring login.
 *
 * This component is presentation only. It cannot be the security boundary:
 * the app ships with `withGlobalTauri: true`, so anything decided here is
 * re-decidable from a devtools console, and the bundle itself can be
 * edited. The actual enforcement is `commands::update_app_config` refusing
 * to flip `onboarding_completed` without a grant recorded in Rust — so
 * skipping past this screen by any front-end route (a direct `#/onboarding`
 * URL, a patched guard) still leaves setup unable to finish.
 */
export function VendorGate() {
  const [status, setStatus] = useState<{ required: boolean; authorized: boolean } | null>(null);
  const [password, setPassword] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void getVendorGateStatus()
      .then((next) => {
        if (!cancelled) setStatus(next);
      })
      .catch(() => {
        // If the status read itself fails there's nothing safe to assume, so
        // hold on the loading state rather than rendering the wizard: the
        // backend would refuse to complete setup anyway, and showing the
        // wizard here would only produce a confusing failure at the end.
        if (!cancelled) setStatus(null);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!password || isSubmitting) return;

    setIsSubmitting(true);
    setError(null);
    try {
      await verifyVendorGate(password);
      setStatus({ required: true, authorized: true });
      setPassword("");
    } catch (err) {
      // The backend has already applied its escalating delay by the time
      // this rejects, so there's no additional client-side throttle here —
      // adding one would only slow an honest vendor, since a scripted
      // attacker would not be running this code anyway.
      setError((err as Error).message);
      setPassword("");
    } finally {
      setIsSubmitting(false);
    }
  };

  if (status === null) return null;
  if (!status.required || status.authorized) return <Outlet />;

  return (
    <div className="flex min-h-dvh items-center justify-center bg-slate-100 p-4">
      <div className="w-full max-w-sm rounded-2xl bg-white p-7 shadow-soft">
        <div className="mb-5 flex flex-col items-center text-center">
          <span className="mb-3 flex h-11 w-11 items-center justify-center rounded-full bg-slate-900">
            <ShieldCheck className="h-5 w-5 text-white" />
          </span>
          <h1 className="text-lg font-semibold text-slate-900">Authorization required</h1>
          <p className="mt-2 text-sm text-slate-600">
            Enter the vendor authorization password to set up this installation.
          </p>
        </div>

        <form onSubmit={submit} className="space-y-3">
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            placeholder="Authorization password"
            autoFocus
            autoComplete="off"
            className="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm focus:border-brand-500 focus:outline-none focus:ring-2 focus:ring-brand-200"
          />

          {error && (
            <p role="alert" className="rounded-md bg-red-50 px-3 py-2 text-sm text-red-700">
              {error}
            </p>
          )}

          <button
            type="submit"
            disabled={!password || isSubmitting}
            className="w-full rounded-lg bg-slate-900 px-4 py-2 text-sm font-medium text-white hover:bg-slate-800 disabled:opacity-50"
          >
            {isSubmitting ? "Checking…" : "Continue"}
          </button>
        </form>

        <p className="mt-5 text-center text-xs text-slate-400">
          This installation has not been authorized yet. Contact your supplier to proceed.
        </p>
      </div>
    </div>
  );
}
