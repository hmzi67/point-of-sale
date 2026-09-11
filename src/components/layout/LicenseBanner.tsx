import { AlertTriangle } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { useAuthStore, useLicenseStore } from "../../store";
import { roleCanAccessSettings } from "../../utils/permissions";
import { formatTimeUntil, formatUnixDateTime } from "../../utils/licenseFormat";

/**
 * The "renew soon" warning: shown on every screen, for every role, from
 * 24 hours before the licence expires (Rust decides — `expiringSoon` is
 * computed in `license::state`, this only renders it). A persistent strip
 * rather than a launch-time modal so it can't be dismissed reflexively
 * and doesn't interrupt a sale in progress.
 *
 * Renders nothing in every other state: the lock screen
 * (`LicenseLockGate`) owns the expired/revoked/etc. cases, and a licence
 * that is fine has nothing to say.
 */
export function LicenseBanner() {
  const state = useLicenseStore((s) => s.state);
  const role = useAuthStore((s) => s.user?.role);
  const navigate = useNavigate();

  if (state?.kind !== "ok" || !state.expiringSoon || state.expiresAt === null) return null;

  const canRenewHere = role !== undefined && roleCanAccessSettings(role);

  return (
    <div
      role="status"
      className="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-warning-line bg-warning-soft px-4 py-2 text-sm text-warning-text"
    >
      <AlertTriangle className="h-4 w-4 shrink-0 text-warning-text" aria-hidden />
      <span className="min-w-0 flex-1">
        <span className="font-semibold">Your licence expires {formatTimeUntil(state.expiresAt)}</span>
        <span className="text-warning-text"> ({formatUnixDateTime(state.expiresAt)}). </span>
        {canRenewHere
          ? "Renew it to avoid interruption."
          : "Ask the owner to renew it to avoid interruption."}
      </span>
      {canRenewHere && (
        <button
          type="button"
          onClick={() => navigate("/settings#license")}
          className="shrink-0 rounded-lg bg-warning px-3 py-1 text-sm font-semibold text-fg hover:bg-warning"
        >
          Renew
        </button>
      )}
    </div>
  );
}
