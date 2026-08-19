import type { ModuleKey, Role } from "../types";

/**
 * The product's permission matrix — two roles, kept deliberately minimal.
 *
 * | Area                          | Owner | Admin | Cashier |
 * |--------------------------------|:-----:|:-----:|:-------:|
 * | Billing (search, sell, receipt)|   ✓   |   ✓   |    ✓    |
 * | Inventory — view stock          |   ✓   |   ✓   |    ✓    |
 * | Inventory — add/edit/delete     |   ✓   |   ✓   |         |
 * | Tables — bill against a table   |   ✓   |   ✓   |    ✓    |
 * | Tables — floor management       |   ✓   |   ✓   |         |
 * | Dashboard                       |   ✓   |   ✓   |         |
 * | Reports                         |   ✓   |   ✓   |         |
 * | Attendance                      |   ✓   |   ✓   |         |
 * | Expenses                        |   ✓   |   ✓   |         |
 * | Salary                          |   ✓   |   ✓   |         |
 * | Settings (module toggles, config)|  ✓   |   ✓   |         |
 * | User management                 |   ✓   |   ✓   |         |
 * | — create/promote another Owner  |   ✓   |       |         |
 * | — deactivate an Owner account   |  ✓*   |       |         |
 *
 * `*` an Owner can deactivate a *different* Owner, never themselves, and
 * never the last active one — see `db::users::set_active`.
 *
 * Owner and Admin are functionally identical everywhere except the two
 * ✓* rows above: this product doesn't yet have a workflow that needs a third
 * tier ("manager", "supervisor", ...), so it doesn't have one — adding a role
 * nobody asked for is exactly the kind of over-building the brief warns
 * against. If a real need for a narrower admin tier shows up later, it slots
 * in here without touching anything module-shaped.
 *
 * This file is the *first* check, used to keep the UI honest (hide a link a
 * role can't use, redirect a direct URL hit). It is never the *only* check:
 * every Tauri command behind a ✗ above re-verifies the caller's role
 * server-side against `Session` (see `src-tauri/src/session.rs`), because a
 * frontend check alone stops a confused user, not a deliberate one poking
 * `invoke()` directly.
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

/** Same tier as Settings — kept as its own function since user management is
 * conceptually its own screen, not a Settings subsection. */
export function roleCanManageUsers(role: Role): boolean {
  return isAdminRole(role);
}

export function isModuleReadOnlyFor(role: Role, key: ModuleKey): boolean {
  return !isAdminRole(role) && CASHIER_READ_ONLY_MODULES.includes(key);
}

/** Only an Owner may create or promote another Owner account — mirrors
 * `commands::create_user`/`update_user` in Rust, which is what actually
 * enforces this; this copy only lets the form disable the option instead of
 * offering a choice the server will just reject. */
export function roleCanAssignRole(actingRole: Role, targetRole: Role): boolean {
  return targetRole !== "owner" || actingRole === "owner";
}
