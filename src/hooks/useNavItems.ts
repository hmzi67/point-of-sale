import { useMemo } from "react";
import type { LucideIcon } from "lucide-react";
import { useModules } from "./useModules";

export interface NavEntry {
  key: string;
  path: string;
  label: string;
  icon: LucideIcon;
  /** `NavLink end` — only the Dashboard's "/" needs exact matching. */
  end: boolean;
  /** Not in the licence's plan: shown with a lock, opens the upgrade notice. */
  locked: boolean;
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

  const items = useMemo<NavEntry[]>(() => {
    const list: NavEntry[] = visibleModules.map((module) => ({
      key: module.key,
      path: module.nav.path,
      label: module.name,
      icon: module.nav.icon,
      end: module.nav.path === "/",
      locked: !module.entitled,
    }));
    return list;
  }, [visibleModules]);

  return { items, isLoaded };
}
