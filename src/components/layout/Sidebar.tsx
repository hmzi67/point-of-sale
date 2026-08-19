import { useState } from "react";
import { NavLink } from "react-router-dom";
import { ChevronLeft, ChevronRight, Store } from "lucide-react";
import { useModules } from "../../hooks/useModules";
import { useAuthStore } from "../../store";
import { SETTINGS_NAV, USERS_NAV } from "../../utils/navigation";
import { roleCanAccessSettings, roleCanManageUsers } from "../../utils/permissions";

const COLLAPSED_KEY = "pos.sidebarCollapsed";

function linkClass(collapsed: boolean) {
  return ({ isActive }: { isActive: boolean }) =>
    [
      "flex items-center gap-3 rounded-md px-3 py-2 text-sm transition-colors",
      collapsed ? "justify-center" : "",
      isActive ? "bg-brand-600 text-white" : "hover:bg-slate-800 hover:text-white",
    ].join(" ");
}

/**
 * Rendered entirely from the module config — there is no hardcoded nav list.
 * Disabling a module in Settings removes its link here on the next render.
 *
 * Foldable: collapsing swaps the 240px labeled rail for a 64px icon-only one
 * (icons stay, labels drop to a `title` tooltip) rather than hiding the
 * sidebar outright, so navigation is always one click away. The choice
 * persists in localStorage — purely a display preference, not worth a round
 * trip to SQLite for.
 */
export function Sidebar() {
  const { visibleModules, isLoaded } = useModules();
  const role = useAuthStore((state) => state.user?.role ?? null);
  const showSettings = role ? roleCanAccessSettings(role) : false;
  const showUsers = role ? roleCanManageUsers(role) : false;

  const [collapsed, setCollapsed] = useState(() => {
    try {
      return localStorage.getItem(COLLAPSED_KEY) === "1";
    } catch {
      return false;
    }
  });

  const toggle = () => {
    setCollapsed((prev) => {
      const next = !prev;
      try {
        localStorage.setItem(COLLAPSED_KEY, next ? "1" : "0");
      } catch {
        // Best-effort only — a private-browsing/quota failure here just
        // means the preference won't survive a reload, nothing worse.
      }
      return next;
    });
  };

  const link = linkClass(collapsed);

  return (
    <aside
      className={[
        "relative flex shrink-0 flex-col border-r border-slate-800 bg-slate-900 text-slate-300 transition-[width] duration-200",
        collapsed ? "w-16" : "w-60",
      ].join(" ")}
    >
      <div className={["flex h-14 items-center gap-2 border-b border-slate-800", collapsed ? "justify-center px-2" : "px-4"].join(" ")}>
        <Store className="h-5 w-5 shrink-0 text-brand-500" />
        {!collapsed && <span className="font-semibold tracking-tight text-white">POS</span>}
      </div>

      <nav className="flex-1 overflow-y-auto p-2">
        {!isLoaded ? (
          !collapsed && <p className="px-3 py-2 text-sm text-slate-500">Loading modules…</p>
        ) : (
          <ul className="space-y-1">
            {visibleModules.map((module) => {
              const Icon = module.nav.icon;
              return (
                <li key={module.key}>
                  <NavLink
                    to={module.nav.path}
                    end={module.nav.path === "/"}
                    className={link}
                    title={collapsed ? module.name : undefined}
                  >
                    <Icon className="h-4 w-4 shrink-0" />
                    {!collapsed && module.name}
                  </NavLink>
                </li>
              );
            })}
          </ul>
        )}
      </nav>

      {(showUsers || showSettings) && (
        <div className="border-t border-slate-800 p-2">
          {showUsers && (
            <NavLink to={USERS_NAV.path} className={link} title={collapsed ? USERS_NAV.label : undefined}>
              <USERS_NAV.icon className="h-4 w-4 shrink-0" />
              {!collapsed && USERS_NAV.label}
            </NavLink>
          )}
          {showSettings && (
            <NavLink to={SETTINGS_NAV.path} className={link} title={collapsed ? SETTINGS_NAV.label : undefined}>
              <SETTINGS_NAV.icon className="h-4 w-4 shrink-0" />
              {!collapsed && SETTINGS_NAV.label}
            </NavLink>
          )}
        </div>
      )}

      <button
        type="button"
        onClick={toggle}
        aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
        title={collapsed ? "Expand sidebar" : "Collapse sidebar"}
        className="flex items-center justify-center gap-2 border-t border-slate-800 py-2.5 text-slate-500 transition-colors hover:bg-slate-800 hover:text-white"
      >
        {collapsed ? <ChevronRight className="h-4 w-4" /> : <ChevronLeft className="h-4 w-4" />}
        {!collapsed && <span className="text-xs">Collapse</span>}
      </button>

      {!collapsed && (
        <div className="border-t border-slate-800 px-4 py-3 text-xs text-slate-500">Offline mode · v0.1.0</div>
      )}
    </aside>
  );
}
