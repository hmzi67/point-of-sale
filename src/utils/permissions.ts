import type { ModuleKey, Role } from "../types";

/**
 * The product's permission matrix.
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
 * | User management (screen access) |   ✓   |   ✓   |         |
 * | Employee management             |   ✓   |   ✓   |         |
 *
 * Owner and Admin see the same *screens*, but User Management itself is
 * hierarchical, not flat — there is exactly one Owner account per
 * installation (seeded at first run) and it outranks Admin:
 *
 * | Can create/edit/deactivate →  | Owner | Admin | Cashier |
 * |--------------------------------|:-----:|:-----:|:-------:|
 * | ...an Owner account            |  n/a  |   ✗   |   ✗     |
 * | ...an Admin account            |   ✓   |   ✗   |   ✗     |
 * | ...a Cashier account           |   ✓   |   ✓   |   ✗     |
 *
 * The Owner may always edit (name/PIN) their *own* account, and its role can
 * never change — but it can never be deactivated, by anyone, including
 * itself. `roleCanManageAccount`/`assignableRoles` below are the frontend
 * mirror of this; `caller_may_manage`/`assignable_roles` in
 * `src-tauri/src/commands.rs` are what actually enforce it.
 *
 * This file is the *first* check, used to keep the UI honest (hide a link or
 * button a role can't use, redirect a direct URL hit). It is never the
 * *only* check: every Tauri command this file gates re-verifies the
 * caller's role server-side against `Session` (see
 * `src-tauri/src/session.rs`), because a frontend check alone stops a
 * confused user, not a deliberate one poking `invoke()` directly.
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

/** Same tier again — employee (payroll/attendance) records are managed on
 * their own screen too, gated the same as Users/Settings rather than tied to
 * the Attendance or Salary module toggle, since either module can consume
 * this list even when the other (or both) are off. */
export function roleCanManageEmployees(role: Role): boolean {
  return isAdminRole(role);
}

export function isModuleReadOnlyFor(role: Role, key: ModuleKey): boolean {
  return !isAdminRole(role) && CASHIER_READ_ONLY_MODULES.includes(key);
}

/** Roles `actingRole` may assign when creating an account or changing an
 * existing one's role — Owner is never offered to anyone, since there is
 * exactly one Owner account per installation (seeded at first run) and it
 * isn't a role this screen hands out. Mirrors `assignable_roles` in
 * `src-tauri/src/commands.rs`, which is what actually enforces it; this copy
 * only keeps the "Add/edit staff account" dropdown from offering a choice
 * the server would reject. */
export function assignableRoles(actingRole: Role): Role[] {
  if (actingRole === "owner") return ["admin", "cashier"];
  if (actingRole === "admin") return ["cashier"];
  return [];
}

/** Whether `actorRole` may edit or deactivate an account whose *current*
 * role is `targetRole` — used for the "own account" exception too, so pass
 * `isSelf` for that case: editing your own non-Owner account is always
 * allowed (deactivating it is refused separately, unconditionally, by the
 * caller — see `UserTable`), and the Owner may edit (never deactivate)
 * *their own* account specifically. Mirrors `caller_may_manage` /
 * `authorize_update_user` / `authorize_set_active` in
 * `src-tauri/src/commands.rs`, which is what actually enforces this. */
export function roleCanManageAccount(actorRole: Role, targetRole: Role, isSelf: boolean): boolean {
  if (targetRole === "owner") return isSelf && actorRole === "owner";
  if (isSelf) return true;
  if (actorRole === "owner") return true;
  if (actorRole === "admin") return targetRole === "cashier";
  return false;
}
