import { call } from "./tauriClient";

/**
 * The shared "sensitive area" PIN — one installation-wide PIN, distinct from
 * any individual staff account's login PIN, that gates Attendance, Expenses,
 * Salary, Employees and Reports for every role (Owner, Admin, Cashier) on
 * every visit. See `AreaPinGate` (the frontend gate that calls these) and
 * `commands::require_area_access` on the Rust side (the actual server-side
 * enforcement, not just a UI hide).
 */

/** Verifies `pin` and, on success, grants server-side access for as long as
 * the caller stays on a gated screen (`AreaPinGate` revokes it on unmount —
 * see that component for why "every visit needs its own PIN" is enforced
 * there, not by this call alone). Throws with a user-facing message on an
 * incorrect PIN. */
export function verifyAreaPin(pin: string): Promise<void> {
  return call<void>("security_verify_area_pin", { pin });
}

/** Ends the current grant immediately. Safe to call even if nothing was
 * granted. */
export function revokeAreaAccess(): Promise<void> {
  return call<void>("security_revoke_area_access", {});
}

/** Changes the shared area PIN — Owner/Admin only server-side (Settings
 * itself is admin-only). Requires the current PIN. */
export function setAreaPin(oldPin: string, newPin: string): Promise<void> {
  return call<void>("security_set_area_pin", { oldPin, newPin });
}
