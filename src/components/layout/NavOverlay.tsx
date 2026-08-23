import { NavLink } from "react-router-dom";
import { LogOut, User, X } from "lucide-react";
import { useModules } from "../../hooks/useModules";
import { useAuthStore } from "../../store";
import { EMPLOYEES_NAV, SETTINGS_NAV, USERS_NAV } from "../../utils/navigation";
import { roleCanAccessSettings, roleCanManageEmployees, roleCanManageUsers } from "../../utils/permissions";

const ROLE_LABEL: Record<string, string> = {
  owner: "Owner",
  admin: "Admin",
  cashier: "Cashier",
};

interface NavOverlayProps {
  open: boolean;
  onClose: () => void;
}

const linkClass = ({ isActive }: { isActive: boolean }) =>
  [
    // min-h-11 (44px): py-2.5 alone comes out a little short of a
    // comfortable phone touch target.
    "flex min-h-11 items-center gap-3 rounded-xl px-3.5 py-2.5 text-sm font-medium transition-colors",
    isActive ? "bg-brand-600 text-white shadow-soft" : "text-slate-600 hover:bg-slate-100",
  ].join(" ");

/**
 * Replaces the old always-visible dark sidebar with a hamburger-triggered
 * overlay — same underlying data source (`useModules()`), just a different
 * container. A disabled module or a role that can't see it still never gets
 * a nav item; nothing about the module-config/permission wiring changed,
 * only how the resulting list is presented.
 *
 * Module labels stay exactly what they are the rest of the app (Settings'
 * own toggle list, page headings) — "Billing", "Reports", etc. — rather than
 * borrowed reference terms like "Point of Sales" that would read as a
 * different feature everywhere else in the product.
 */
export function NavOverlay({ open, onClose }: NavOverlayProps) {
  const { visibleModules, isLoaded } = useModules();
  const user = useAuthStore((state) => state.user);
  const logout = useAuthStore((state) => state.logout);
  const role = user?.role ?? null;
  const showSettings = role ? roleCanAccessSettings(role) : false;
  const showUsers = role ? roleCanManageUsers(role) : false;
  const showEmployees = role ? roleCanManageEmployees(role) : false;

  return (
    // h-dvh (not relying on inset-0's implicit sizing, and not h-screen/vh):
    // dvh tracks the WebView's actual visible viewport, which is the more
    // reliable unit for this on Android per the platform's own guidance —
    // 100vh has a long history of mismatching the real visible area on
    // mobile browsers/webviews, dvh doesn't.
    <div
      className={[
        "fixed inset-0 z-50 h-dvh transition-opacity duration-200",
        open ? "pointer-events-auto opacity-100" : "pointer-events-none opacity-0",
      ].join(" ")}
      aria-hidden={!open}
    >
      {/* Backdrop — click outside the panel to close. */}
      <button
        type="button"
        aria-label="Close navigation"
        onClick={onClose}
        className="absolute inset-0 bg-slate-900/30"
        tabIndex={open ? 0 : -1}
      />

      <aside
        className={[
          "relative flex h-full w-[300px] max-w-[85vw] flex-col bg-white shadow-soft-lg transition-transform duration-200",
          open ? "translate-x-0" : "-translate-x-full",
        ].join(" ")}
      >
        {/* pt adds the status-bar/notch inset on top of the row's own
            padding — same edge-to-edge reasoning as TopBar. This overlay
            covers the full screen (including behind the status bar), so it
            needs its own inset handling independent of TopBar's. */}
        <div className="flex items-center justify-between border-b border-slate-100 px-5 pb-4 pt-[calc(env(safe-area-inset-top)+1rem)]">
          {user && (
            <div className="flex items-center gap-2.5">
              <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-brand-100 text-brand-700">
                <User className="h-4 w-4" />
              </span>
              <span className="min-w-0">
                <span className="block truncate text-sm font-semibold text-slate-900">{user.name}</span>
                <span className="block text-xs text-slate-500">{ROLE_LABEL[user.role] ?? user.role}</span>
              </span>
            </div>
          )}
          <button
            type="button"
            onClick={onClose}
            aria-label="Close navigation"
            className="flex min-h-11 min-w-11 items-center justify-center rounded-full text-slate-400 hover:bg-slate-100 hover:text-slate-600"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <nav className="flex-1 overflow-y-auto px-3 py-3">
          {!isLoaded ? (
            <p className="px-3.5 py-2 text-sm text-slate-400">Loading…</p>
          ) : (
            <ul className="space-y-1">
              {visibleModules.map((module) => {
                const Icon = module.nav.icon;
                return (
                  <li key={module.key}>
                    <NavLink to={module.nav.path} end={module.nav.path === "/"} className={linkClass} onClick={onClose}>
                      <Icon className="h-4 w-4 shrink-0" />
                      {module.name}
                    </NavLink>
                  </li>
                );
              })}
              {showEmployees && (
                <li>
                  <NavLink to={EMPLOYEES_NAV.path} className={linkClass} onClick={onClose}>
                    <EMPLOYEES_NAV.icon className="h-4 w-4 shrink-0" />
                    {EMPLOYEES_NAV.label}
                  </NavLink>
                </li>
              )}
              {showUsers && (
                <li>
                  <NavLink to={USERS_NAV.path} className={linkClass} onClick={onClose}>
                    <USERS_NAV.icon className="h-4 w-4 shrink-0" />
                    {USERS_NAV.label}
                  </NavLink>
                </li>
              )}
              {showSettings && (
                <li>
                  <NavLink to={SETTINGS_NAV.path} className={linkClass} onClick={onClose}>
                    <SETTINGS_NAV.icon className="h-4 w-4 shrink-0" />
                    {SETTINGS_NAV.label}
                  </NavLink>
                </li>
              )}
            </ul>
          )}
        </nav>

        {/* pb adds the gesture-nav-bar inset on top of the section's own
            padding, same pattern BottomTabBar already uses. */}
        <div className="border-t border-slate-100 p-3 pb-[calc(env(safe-area-inset-bottom)+0.75rem)]">
          <button
            type="button"
            onClick={() => {
              onClose();
              logout();
            }}
            className="flex min-h-11 w-full items-center gap-3 rounded-xl px-3.5 py-2.5 text-sm font-medium text-red-600 hover:bg-red-50"
          >
            <LogOut className="h-4 w-4" />
            Log Out
          </button>
        </div>
      </aside>
    </div>
  );
}
