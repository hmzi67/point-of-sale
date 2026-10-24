import type { ReactNode } from "react";
import { Navigate } from "react-router-dom";
import { useModules } from "../../hooks/useModules";
import { useAuthStore } from "../../store";
import { isAdminRole } from "../../utils/permissions";
import { UpgradeNotice } from "../ui/UpgradeNotice";
import type { ModuleKey } from "../../types";

interface ModuleRouteProps {
  /** Omit for admin-only screens that are not modules (Settings, Users). */
  moduleKey?: ModuleKey;
  adminOnly?: boolean;
  children: ReactNode;
}

/**
 * Route guard combining module configuration with role.
 *
 * A URL typed directly into the address bar goes through exactly the same check
 * as a sidebar click, so hiding a module in Settings also blocks navigation to
 * it — that is the whole point of the config-driven architecture.
 */
export function ModuleRoute({ moduleKey, adminOnly = false, children }: ModuleRouteProps) {
  const { canAccess, lockedModule, fallbackPath, isLoaded } = useModules();
  const role = useAuthStore((state) => state.user?.role ?? null);

  // Redirecting before the config has loaded would bounce a valid deep link.
  if (!isLoaded || !role) return null;

  if (adminOnly && !isAdminRole(role)) {
    return <Navigate to={fallbackPath} replace />;
  }

  // Enabled and allowed for this role, but not in the licence's plan: say
  // so (and which plan has it) rather than silently bouncing elsewhere.
  const locked = moduleKey ? lockedModule(moduleKey) : null;
  if (locked) {
    return <UpgradeNotice feature={locked.name} requiredPlan={locked.requiredPlan} />;
  }

  if (moduleKey && !canAccess(moduleKey)) {
    return <Navigate to={fallbackPath} replace />;
  }

  return <>{children}</>;
}
