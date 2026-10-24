import { useEffect, useRef, useState, type FormEvent } from "react";
import { BadgeCheck, RefreshCw, ShieldAlert } from "lucide-react";
import { useLocation } from "react-router-dom";
import { useLicenseStore } from "../../store";
import { formatTimeUntil, formatUnixDateTime } from "../../utils/licenseFormat";
import type { LicenseState } from "../../types";

/**
 * Settings › Licence. Shows the current state as Rust computed it, lets
 * the owner re-check with the server, and lets them enter/replace the
 * licence key — the "moved to a new device after support approved it"
 * path, and the renewal path (renewal happens server-side; "Re-check now"
 * is what pulls the new expiry down).
 *
 * Settings is Owner/Admin-only already (route guard) and `license_activate`
 * re-checks that in Rust, so there is no role check here to duplicate.
 * The banner's "Renew" button and the lock screen both deep-link here
 * with `#license`. `SettingsPage` reads that same hash as its active tab,
 * so the link lands on the Licence tab (mounting this section); the
 * scroll below then brings it into view — currently a near no-op since
 * it's that tab's only card, kept so the link still works if more join.
 */
export function LicenseSection() {
  const state = useLicenseStore((s) => s.state);
  const info = useLicenseStore((s) => s.info);
  const plan = useLicenseStore((s) => s.plan);
  const isBusy = useLicenseStore((s) => s.isBusy);
  const loadInfo = useLicenseStore((s) => s.loadInfo);
  const refresh = useLicenseStore((s) => s.refresh);
  const activate = useLicenseStore((s) => s.activate);

  const [key, setKey] = useState("");
  const [showKeyForm, setShowKeyForm] = useState(false);
  const [message, setMessage] = useState<{ tone: "ok" | "warn" | "error"; text: string } | null>(null);
  const sectionRef = useRef<HTMLDivElement>(null);
  const { hash } = useLocation();

  useEffect(() => {
    void loadInfo();
  }, [loadInfo]);

  useEffect(() => {
    if (hash === "#license") sectionRef.current?.scrollIntoView({ behavior: "smooth", block: "start" });
  }, [hash]);

  // No key stored yet: the form is the whole point, show it straight away.
  const keyMissing = state?.kind === "missing";
  const formOpen = showKeyForm || keyMissing;

  const onRefresh = async () => {
    setMessage(null);
    try {
      const hint = await refresh();
      setMessage(
        hint
          ? { tone: "warn", text: `Couldn't reach the licence server — ${hint}. Showing the last verified status.` }
          : { tone: "ok", text: "Licence status is up to date." },
      );
    } catch (err) {
      setMessage({ tone: "error", text: (err as Error).message });
    }
  };

  const onActivate = async (e: FormEvent) => {
    e.preventDefault();
    if (!key.trim() || isBusy) return;
    setMessage(null);
    try {
      await activate(key);
      setKey("");
      setShowKeyForm(false);
      setMessage({ tone: "ok", text: "Licence activated on this device." });
    } catch (err) {
      setMessage({ tone: "error", text: (err as Error).message });
    }
  };

  return (
    <div ref={sectionRef} id="license" className="scroll-mt-4 rounded-2xl border border-line bg-surface p-6">
      <h2 className="flex items-center gap-2 text-base font-semibold tracking-tight text-fg">
        <BadgeCheck className="h-4 w-4 text-fg-muted" />
        Licence
      </h2>
      <p className="mt-1 text-sm text-fg-2">
        This installation's licence is verified on every launch, including offline. Renewals made
        on the server are picked up automatically, or straight away with <em>Re-check now</em>.
      </p>

      <dl className="mt-4 grid grid-cols-1 gap-4 text-sm sm:grid-cols-2">
        <div>
          <dt className="text-fg-muted">Status</dt>
          <dd className="mt-0.5">{state ? <StatusLine state={state} /> : <span className="text-fg-muted">Loading…</span>}</dd>
        </div>
        <div>
          <dt className="text-fg-muted">Plan</dt>
          <dd className="mt-0.5 text-fg">
            {plan?.planName ?? (plan && !plan.restricted ? "Full access (licence issued before plans)" : "—")}
          </dd>
        </div>
        <div>
          <dt className="text-fg-muted">Licence key</dt>
          <dd className="mt-0.5 font-mono text-fg">{info?.keyPrefix ?? "—"}</dd>
        </div>
        <div>
          <dt className="text-fg-muted">Last checked with server</dt>
          <dd className="mt-0.5 text-fg">
            {info?.lastOnlineCheck ? formatUnixDateTime(info.lastOnlineCheck) : "Never"}
          </dd>
        </div>
        <div>
          <dt className="text-fg-muted">Licence server</dt>
          <dd className="mt-0.5 truncate text-fg" title={info?.apiBase}>
            {info?.apiBase ?? "—"}
          </dd>
        </div>
      </dl>

      <div className="mt-4 flex flex-wrap gap-2">
        <button
          type="button"
          onClick={() => void onRefresh()}
          disabled={isBusy || keyMissing}
          className="inline-flex items-center gap-1.5 rounded-xl border border-line-strong px-3 py-2.5 text-sm font-medium text-fg-2 hover:bg-surface-2 disabled:opacity-50"
        >
          <RefreshCw className={`h-3.5 w-3.5 ${isBusy ? "animate-spin" : ""}`} />
          Re-check now
        </button>
        {!keyMissing && (
          <button
            type="button"
            onClick={() => {
              setShowKeyForm((open) => !open);
              setMessage(null);
            }}
            className="rounded-xl border border-line-strong px-3 py-2.5 text-sm font-medium text-fg-2 hover:bg-surface-2"
          >
            {showKeyForm ? "Cancel" : "Change licence key"}
          </button>
        )}
      </div>

      {formOpen && (
        <form onSubmit={(e) => void onActivate(e)} className="mt-4 grid max-w-md gap-3">
          <label className="block">
            <span className="mb-1 block text-xs font-medium text-fg-muted">
              {keyMissing ? "Licence key" : "New licence key"}
            </span>
            <input
              type="text"
              autoCapitalize="characters"
              autoCorrect="off"
              spellCheck={false}
              placeholder="DIWAN-XXXX-XXXX-XXXX-XXXX"
              value={key}
              onChange={(e) => setKey(e.target.value.toUpperCase())}
              className="w-full rounded-xl border border-line-strong px-2.5 py-1.5 font-mono text-sm uppercase focus:border-accent focus:outline-none"
            />
          </label>
          <p className="text-xs text-fg-muted">
            Activating binds the key to this device and needs an internet connection. A key already
            in use on another device is refused — contact support to move it first.
          </p>
          <div>
            <button
              type="submit"
              disabled={isBusy || key.trim().length < 10}
              className="rounded-xl bg-accent px-4 py-1.5 text-sm font-semibold text-accent-ink hover:bg-accent-hover disabled:opacity-50"
            >
              {isBusy ? "Activating…" : "Activate on this device"}
            </button>
          </div>
        </form>
      )}

      {message && (
        <p
          role="status"
          className={`mt-3 text-sm ${
            message.tone === "ok" ? "text-success-text" : message.tone === "warn" ? "text-warning-text" : "text-danger-text"
          }`}
        >
          {message.text}
        </p>
      )}
    </div>
  );
}

/** One line per state, matching the wording the lock screen uses. */
function StatusLine({ state }: { state: LicenseState }) {
  switch (state.kind) {
    case "ok":
      if (state.expiresAt === null) return <Good>Active — does not expire</Good>;
      return state.expiringSoon ? (
        <Bad tone="warn">
          Expires {formatTimeUntil(state.expiresAt)} ({formatUnixDateTime(state.expiresAt)})
        </Bad>
      ) : (
        <Good>Active until {formatUnixDateTime(state.expiresAt)}</Good>
      );
    case "expired":
      return <Bad>Expired{state.expiredAt ? ` on ${formatUnixDateTime(state.expiredAt)}` : ""}</Bad>;
    case "revoked":
      return <Bad>Revoked</Bad>;
    case "suspended":
      return <Bad>Suspended — contact the Diwan team</Bad>;
    case "deviceMismatch":
      return <Bad>Bound to a different device</Bad>;
    case "clockTampered":
      return <Bad>Device clock is behind — correct it, or re-check online</Bad>;
    case "checkInRequired":
      return <Bad>Offline for more than {state.maxOfflineDays} days — re-check online</Bad>;
    case "invalid":
      return <Bad>{state.message}</Bad>;
    case "missing":
      return <Bad>No licence on this device</Bad>;
  }
}

function Good({ children }: { children: React.ReactNode }) {
  return <span className="font-medium text-success-text">{children}</span>;
}

function Bad({ children, tone = "error" }: { children: React.ReactNode; tone?: "error" | "warn" }) {
  return (
    <span className={`inline-flex items-center gap-1 font-medium ${tone === "warn" ? "text-warning-text" : "text-danger-text"}`}>
      <ShieldAlert className="h-3.5 w-3.5" />
      {children}
    </span>
  );
}
