import type { ModuleKey, Role } from "../types";

/**
 * Role rules, applied on top of module configuration — never instead of it.
 * A screen is reachable only when its module is enabled for this installation
 * AND the signed-in role is allowed to see it.
 */

/** Cashiers get the till and a look at stock; nothing else. */
const CASHIER_MODULES: ModuleKey[] = ["billing", "inventory"];

/** Modules a cashier may open but must not edit. */
const CASHIER_READ_ONLY_MODULES: ModuleKey[] = ["inventory"];

export function isAdminRole(role: Role): boolean {
  return role === "owner" || role === "admin";
}

export function roleCanAccessModule(role: Role, key: ModuleKey): boolean {
  return isAdminRole(role) || CASHIER_MODULES.includes(key);
}

/** Settings configures the modules, so it is admin-only and never toggleable. */
export function roleCanAccessSettings(role: Role): boolean {
  return isAdminRole(role);
}

export function isModuleReadOnlyFor(role: Role, key: ModuleKey): boolean {
  return !isAdminRole(role) && CASHIER_READ_ONLY_MODULES.includes(key);
}
