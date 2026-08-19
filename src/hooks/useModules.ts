import { useMemo } from "react";
import { useAuthStore, useModuleStore } from "../store";
import { MODULE_NAV, type NavItem } from "../utils/navigation";
import { roleCanAccessModule } from "../utils/permissions";
import type { ModuleKey, ModuleState } from "../types";

export interface VisibleModule extends ModuleState {
  nav: NavItem;
}

/**
 * The single source of truth for "what can the current user see?".
 *
 * Combines the installation's enabled modules with the signed-in role. The
 * sidebar, the route guard and (later) the dashboard cards all read this, so
 * they can never disagree about whether a module is available.
 */
export function useModules() {
  const modules = useModuleStore((state) => state.modules);
  const isLoaded = useModuleStore((state) => state.isLoaded);
  const isLoading = useModuleStore((state) => state.isLoading);
  const error = useModuleStore((state) => state.error);
  const role = useAuthStore((state) => state.user?.role ?? null);

  const visibleModules = useMemo<VisibleModule[]>(() => {
    if (!role) return [];
    return modules
      .filter((module) => module.enabled)
      .filter((module) => roleCanAccessModule(role, module.key))
      .filter((module) => Boolean(MODULE_NAV[module.key]))
      .map((module) => ({ ...module, nav: MODULE_NAV[module.key] }));
  }, [modules, role]);

  const canAccess = useMemo(() => {
    const allowed = new Set(visibleModules.map((module) => module.key));
    return (key: ModuleKey) => allowed.has(key);
  }, [visibleModules]);

  /**
   * Where to send someone who lands on a screen they may not see. Billing is
   * core and always enabled, so this list is never empty for any role — which
   * is what stops a guard redirect from looping.
   */
  const fallbackPath = visibleModules[0]?.nav.path ?? "/settings";

  return { modules, visibleModules, canAccess, fallbackPath, isLoaded, isLoading, error };
}
