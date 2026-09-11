import { useMemo } from "react";
import type { LucideIcon } from "lucide-react";
import { useAuthStore } from "../store";
import { EMPLOYEES_NAV, SETTINGS_NAV, USERS_NAV } from "../utils/navigation";
import { canAttemptEmployeesScreen, roleCanAccessSettings, roleCanManageUsers } from "../utils/permissions";
import { useModules } from "./useModules";

export interface NavEntry {
  key: string;
  path: string;
  label: string;
  icon: LucideIcon;
  /** `NavLink end` — only the Dashboard's "/" needs exact matching. */
  end: boolean;
}

/**
 * The complete, ordered, role-filtered navigation list — modules from
 * `useModules()` (enabled ∩ role), then the three non-module screens under
 * their own permission checks. Both navigation containers (the desktop
 * `Sidebar` and Android's `NavOverlay`) render exactly this, so they cannot
 * disagree about what a given role sees.
 */
export function useNavItems() {
  const { visibleModules, isLoaded } = useModules();
  const role = useAuthStore((state) => state.user?.role ?? null);

  const items = useMemo<NavEntry[]>(() => {
    const list: NavEntry[] = visibleModules.map((module) => ({
      key: module.key,
      path: module.nav.path,
      label: module.name,
      icon: module.nav.icon,
      end: module.nav.path === "/",
    }));
    if (role && canAttemptEmployeesScreen()) list.push({ key: "employees", end: false, ...EMPLOYEES_NAV });
    if (role && roleCanManageUsers(role)) list.push({ key: "users", end: false, ...USERS_NAV });
    if (role && roleCanAccessSettings(role)) list.push({ key: "settings", end: false, ...SETTINGS_NAV });
    return list;
  }, [visibleModules, role]);

  return { items, isLoaded };
}
