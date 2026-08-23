import { NavLink } from "react-router-dom";
import { useModules } from "../../hooks/useModules";

const linkClass = ({ isActive }: { isActive: boolean }) =>
  [
    "flex min-w-[64px] flex-1 flex-col items-center justify-center gap-1 py-1.5 text-[11px] font-medium transition-colors",
    isActive ? "text-brand-600" : "text-slate-400",
  ].join(" ");

/**
 * Android's primary navigation — a fixed bottom tab bar, in place of (well,
 * alongside — the hamburger overlay still covers Settings/Users/Employees/
 * Log Out, unchanged) the desktop-only always-hamburger pattern. Only
 * rendered when `IS_ANDROID` (see `AppLayout`); desktop keeps `NavOverlay`
 * as its one and only nav surface.
 *
 * Reads the exact same `useModules().visibleModules` the hamburger overlay
 * does — same module-config (`android_enabled`) and role filtering, so a
 * module disabled for this platform, or not permitted for this role, simply
 * never gets a tab. Nothing here hardcodes which modules appear; the
 * Phase 12 mobile scoping (Billing/Dashboard/Reports prioritized, Tables/
 * full Inventory editing deprioritized) is entirely encoded in each
 * module's `android_enabled` default, not in this component.
 */
export function BottomTabBar() {
  const { visibleModules, isLoaded } = useModules();

  if (!isLoaded || visibleModules.length === 0) return null;

  return (
    <nav
      className="flex shrink-0 items-stretch overflow-x-auto border-t border-slate-100 bg-white pb-[env(safe-area-inset-bottom)]"
      aria-label="Primary"
    >
      {visibleModules.map((module) => {
        const Icon = module.nav.icon;
        return (
          <NavLink key={module.key} to={module.nav.path} end={module.nav.path === "/"} className={linkClass}>
            <Icon className="h-5 w-5" />
            <span className="max-w-[72px] truncate">{module.name}</span>
          </NavLink>
        );
      })}
    </nav>
  );
}
