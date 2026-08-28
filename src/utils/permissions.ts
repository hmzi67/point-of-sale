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
 * | Reports                         |  PIN  |  PIN  |   PIN   |
 * | Attendance                      |  PIN  |  PIN  |   PIN   |
 * | Expenses                        |  PIN  |  PIN  |   PIN   |
 * | Salary                          |  PIN  |  PIN  |   PIN   |
 * | Employee management             |  PIN  |  PIN  |   PIN   |
 * | Settings (module toggles, config)|  ✓   |   ✓   |         |
 * | User management (screen access) |   ✓   |   ✓   |         |
 *
 * "PIN" rows are gated by the shared "sensitive area" PIN
 * (`AreaPinGate`/`db::security_pin` on the Rust side), not by role — Owner,
 * Admin and Cashier all pass the exact same server-side check
 * (`require_area_access` in `commands.rs`) once they've entered the correct
 * PIN, required fresh on every visit (no session persistence). This is a
 * deliberate departure from every other row in this table, which
 * `roleCanAccessModule`/`isAdminRole` below still gate by role alone — the
 * PIN *is* the authorization for these five, not an extra layer on top of
 * one. Changing the PIN itself is still Owner/Admin only (Settings).
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

/** Cashiers get the till and a look at stock unconditionally. Attendance,
 * expenses, salary and reports are also reachable — but only in the sense
 * that the route/nav link isn't hidden; actually opening one still requires
 * the shared area PIN (`AreaPinGate`), which every role goes through
 * equally. See this file's doc comment's "PIN" rows. */
const CASHIER_MODULES: ModuleKey[] = ["billing", "inventory", "attendance", "expenses", "salary", "reports"];

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

/** Employee (payroll/attendance) records have their own screen, not tied to
 * the Attendance or Salary module toggle (either module can consume this
 * list even when the other, or both, are off) — and unlike Settings/Users
 * above, it's gated by the shared area PIN (`AreaPinGate`), not by role, so
 * every role gets the nav link; actually opening it still requires the PIN.
 * See this file's doc comment's "PIN" rows. */
export function canAttemptEmployeesScreen(): boolean {
  return true;
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
