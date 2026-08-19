import { NavLink } from "react-router-dom";
import { Store } from "lucide-react";
import { useModules } from "../../hooks/useModules";
import { useAuthStore } from "../../store";
import { SETTINGS_NAV } from "../../utils/navigation";
import { roleCanAccessSettings } from "../../utils/permissions";

const linkClass = ({ isActive }: { isActive: boolean }) =>
  [
    "flex items-center gap-3 rounded-md px-3 py-2 text-sm transition-colors",
    isActive ? "bg-brand-600 text-white" : "hover:bg-slate-800 hover:text-white",
  ].join(" ");

/**
 * Rendered entirely from the module config — there is no hardcoded nav list.
 * Disabling a module in Settings removes its link here on the next render.
 */
export function Sidebar() {
  const { visibleModules, isLoaded } = useModules();
  const role = useAuthStore((state) => state.user?.role ?? null);
  const showSettings = role ? roleCanAccessSettings(role) : false;

  return (
    <aside className="flex w-60 shrink-0 flex-col border-r border-slate-800 bg-slate-900 text-slate-300">
      <div className="flex h-14 items-center gap-2 border-b border-slate-800 px-4">
        <Store className="h-5 w-5 text-brand-500" />
        <span className="font-semibold tracking-tight text-white">POS</span>
      </div>

      <nav className="flex-1 overflow-y-auto p-2">
        {!isLoaded ? (
          <p className="px-3 py-2 text-sm text-slate-500">Loading modules…</p>
        ) : (
          <ul className="space-y-1">
            {visibleModules.map((module) => {
              const Icon = module.nav.icon;
              return (
                <li key={module.key}>
                  <NavLink to={module.nav.path} end={module.nav.path === "/"} className={linkClass}>
                    <Icon className="h-4 w-4" />
                    {module.name}
                  </NavLink>
                </li>
              );
            })}
          </ul>
        )}
      </nav>

      {showSettings && (
        <div className="border-t border-slate-800 p-2">
          <NavLink to={SETTINGS_NAV.path} className={linkClass}>
            <SETTINGS_NAV.icon className="h-4 w-4" />
            {SETTINGS_NAV.label}
          </NavLink>
        </div>
      )}

      <div className="border-t border-slate-800 px-4 py-3 text-xs text-slate-500">
        Offline mode · v0.1.0
      </div>
    </aside>
  );
}
