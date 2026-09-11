import { useEffect, useState } from "react";
import { Lock, RefreshCw, Settings as SettingsIcon } from "lucide-react";
import { Outlet, useLocation, useNavigate } from "react-router-dom";
import { useAuthStore, useLicenseStore } from "../../store";
import { roleCanAccessSettings } from "../../utils/permissions";
import { formatUnixDateTime } from "../../utils/licenseFormat";
import type { LicenseState } from "../../services/licenseService";

/**
 * The licence lockout. Sits inside `AppLayout` around the module outlet:
 * while the licence is in any non-OK state, every module screen is
 * replaced by this — but the top bar, navigation, sign-out and (for
 * Owner/Admin) Settings stay reachable, so the person in front of the
 * till can always see *why* it stopped and *how* to fix it. A bricked
 * app with no way to read the renewal instructions helps nobody.
 *
 * Presentation only, like every guard in this app: the actual refusal to
 * record a sale/refund/token lives in Rust (`commands::require_license`),
 * so bypassing this component changes nothing.
 *
 * `state === null` (not read yet) renders the outlet, not the lock — the
 * first read is a local IPC call that resolves in milliseconds, and
 * flashing a lock screen at every launch would train people to ignore it.
 */
export function LicenseLockGate() {
  const state = useLicenseStore((s) => s.state);
  const role = useAuthStore((s) => s.user?.role);
  const { pathname } = useLocation();

  if (state === null || state.kind === "ok") return <Outlet />;

  // Settings is where the key gets entered/renewed; it must stay open for
  // whoever is allowed in there. Everything else is locked.
  const isSettings = pathname.startsWith("/settings");
  if (isSettings && role !== undefined && roleCanAccessSettings(role)) return <Outlet />;

  return <LockScreen state={state} />;
}

type LockedState = Exclude<LicenseState, { kind: "ok" }>;

function LockScreen({ state }: { state: LockedState }) {
  const info = useLicenseStore((s) => s.info);
  const isBusy = useLicenseStore((s) => s.isBusy);
  const loadInfo = useLicenseStore((s) => s.loadInfo);
  const refresh = useLicenseStore((s) => s.refresh);
  const role = useAuthStore((s) => s.user?.role);
  const navigate = useNavigate();
  const [message, setMessage] = useState<{ tone: "warn" | "error"; text: string } | null>(null);

  useEffect(() => {
    void loadInfo();
  }, [loadInfo]);

  const canFixHere = role !== undefined && roleCanAccessSettings(role);
  const { title, detail, nextStep } = describe(state, canFixHere);

  const onRefresh = async () => {
    setMessage(null);
    try {
      const hint = await refresh();
      // If the state flipped to OK the gate above unmounts us; otherwise
      // tell the user the check happened and changed nothing.
      if (hint) setMessage({ tone: "warn", text: `Couldn't reach the licence server — ${hint}.` });
      else setMessage({ tone: "warn", text: "Checked with the server — the licence is still not active." });
    } catch (err) {
      setMessage({ tone: "error", text: (err as Error).message });
    }
  };

  return (
    <div className="mx-auto flex max-w-lg flex-col items-center py-10 text-center">
      <div className="flex h-14 w-14 items-center justify-center rounded-full bg-danger-soft">
        <Lock className="h-7 w-7 text-danger-text" aria-hidden />
      </div>
      <h1 className="mt-4 text-xl font-semibold text-fg">{title}</h1>
      <p className="mt-2 text-sm text-fg-2">{detail}</p>

      <dl className="mt-6 grid w-full grid-cols-2 gap-3 rounded-lg border border-line bg-surface p-4 text-left text-sm">
        <div>
          <dt className="text-fg-muted">Licence key</dt>
          <dd className="font-mono text-fg">{info?.keyPrefix ?? "—"}</dd>
        </div>
        <div>
          <dt className="text-fg-muted">Last checked with server</dt>
          <dd className="text-fg">{info?.lastOnlineCheck ? formatUnixDateTime(info.lastOnlineCheck) : "Never"}</dd>
        </div>
      </dl>

      <p className="mt-6 text-sm font-medium text-fg-2">{nextStep}</p>

      <div className="mt-4 flex flex-wrap justify-center gap-2">
        {state.kind !== "missing" && (
          <button
            type="button"
            onClick={() => void onRefresh()}
            disabled={isBusy}
            className="inline-flex items-center gap-1.5 rounded-md bg-accent px-4 py-2 text-sm font-semibold text-accent-ink hover:bg-accent-hover disabled:opacity-50"
          >
            <RefreshCw className={`h-4 w-4 ${isBusy ? "animate-spin" : ""}`} />
            Re-check now
          </button>
        )}
        {canFixHere && (
          <button
            type="button"
            onClick={() => navigate("/settings#license")}
            className="inline-flex items-center gap-1.5 rounded-md border border-line-strong bg-surface px-4 py-2 text-sm font-medium text-fg-2 hover:bg-surface-2"
          >
            <SettingsIcon className="h-4 w-4" />
            Licence settings
          </button>
        )}
      </div>

      {message && (
        <p role="status" className={`mt-4 text-sm ${message.tone === "warn" ? "text-warning-text" : "text-danger-text"}`}>
          {message.text}
        </p>
      )}

      <p className="mt-8 text-xs text-fg-muted">
        Billing, refunds and kitchen tokens are paused until the licence is active again. Your data
        is safe and nothing has been deleted.
      </p>
    </div>
  );
}

/** Reason-specific copy. `canFixHere` switches the call to action between
 * "do it" (Owner/Admin) and "ask the owner" (Cashier). */
function describe(state: LockedState, canFixHere: boolean) {
  const renew = canFixHere
    ? "Renew the licence with your Diwan vendor, then press Re-check now."
    : "Ask the owner to renew the licence, then press Re-check now.";
  const support = canFixHere
    ? "Contact your Diwan vendor to resolve this."
    : "Ask the owner to contact the Diwan vendor.";

  switch (state.kind) {
    case "expired":
      return {
        title: "Licence expired",
        detail: state.expiredAt
          ? `This installation's licence expired on ${formatUnixDateTime(state.expiredAt)}.`
          : "This installation's licence has expired.",
        nextStep: renew,
      };
    case "revoked":
      return {
        title: "Licence revoked",
        detail: "This licence has been revoked by the vendor.",
        nextStep: support,
      };
    case "deviceMismatch":
      return {
        title: "Licence belongs to another device",
        detail: "The licence stored here was issued to a different computer.",
        nextStep: canFixHere
          ? "Enter the licence key for this device in Licence settings, or contact your vendor to move it."
          : "Ask the owner to enter the correct licence key for this device.",
      };
    case "clockTampered":
      return {
        title: "Device clock is wrong",
        detail:
          state.reason === "behindServerTime"
            ? "This computer's clock is set earlier than the time the licence was issued."
            : "This computer's clock has been set back since the app last ran.",
        nextStep: "Correct the date and time in the system settings, or connect to the internet and press Re-check now.",
      };
    case "checkInRequired":
      return {
        title: "Licence check-in required",
        detail: `This installation has been offline for more than ${state.maxOfflineDays} days and needs to confirm its licence with the server.`,
        nextStep: "Connect to the internet and press Re-check now.",
      };
    case "missing":
      return {
        title: "No licence on this device",
        detail: "This installation has not been activated.",
        nextStep: canFixHere
          ? "Enter your licence key in Licence settings."
          : "Ask the owner to enter the licence key in Settings.",
      };
    case "invalid":
      return {
        title: "Licence could not be verified",
        detail: state.message,
        nextStep: canFixHere
          ? "Connect to the internet and press Re-check now, or re-enter the licence key in Licence settings."
          : "Ask the owner to re-check the licence.",
      };
  }
}
