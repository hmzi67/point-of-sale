import { ShieldAlert } from "lucide-react";
import { useEffect, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { getDefaultCredentials } from "../../services/securityPinService";
import { useAuthStore } from "../../store";
import type { DefaultCredentials } from "../../types";
import { isAdminRole } from "../../utils/permissions";

/**
 * Warns Owner/Admin, on every screen, while the Owner login PIN or the
 * shared area PIN is still the value every install ships with. Rust decides
 * (`security_get_default_credentials` checks the stored hashes); this only
 * renders it. Re-checked on every navigation, so it disappears as soon as
 * the PIN is changed and the user moves on. Not dismissable on purpose — a
 * known PIN on a till that takes money is worth a permanent nag.
 */
export function DefaultPinBanner() {
  const role = useAuthStore((s) => s.user?.role);
  const location = useLocation();
  const navigate = useNavigate();
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

  const which = [
    defaults.ownerPinIsDefault && "the Owner login PIN",
    defaults.areaPinIsDefault && "the protected-area PIN",
  ].filter(Boolean);

  return (
    <div
      role="alert"
      className="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-danger-line bg-danger-soft px-4 py-2 text-sm text-danger-text"
    >
      <ShieldAlert className="h-4 w-4 shrink-0" aria-hidden />
      <span className="min-w-0 flex-1">
        <span className="font-semibold">Default PIN in use.</span> Change {which.join(" and ")} — anyone who knows
        the defaults can sign in or open protected areas.
      </span>
      {/* Only the Owner can change the Owner's PIN — an Admin is outranked. */}
      {defaults.ownerPinIsDefault && role === "owner" && (
        <button
          type="button"
          onClick={() => navigate("/users")}
          className="shrink-0 rounded-lg border border-danger-line px-3 py-1 text-sm font-semibold hover:bg-surface-2"
        >
          Change Owner PIN
        </button>
      )}
      {defaults.areaPinIsDefault && (
        <button
          type="button"
          onClick={() => navigate("/settings#security")}
          className="shrink-0 rounded-lg border border-danger-line px-3 py-1 text-sm font-semibold hover:bg-surface-2"
        >
          Change area PIN
        </button>
      )}
    </div>
  );
}
