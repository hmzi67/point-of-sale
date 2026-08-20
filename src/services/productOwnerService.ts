import type { ModuleState, Platform } from "../types";
import { call } from "./tauriClient";

/**
 * The vendor/developer account's IPC surface — never imported by any
 * component reachable through normal navigation; only by the hidden entry
 * point's modal. See `db::product_owner` (Rust) for the full rationale.
 */

/** Whether a credential has already been set on this install. */
export function getStatus(): Promise<boolean> {
  return call<boolean>("product_owner_get_status", {});
}

/** Sets the initial credential for this install and grants an elevated
 * session immediately. Rejects if one is already set. */
export function setup(password: string): Promise<void> {
  return call<void>("product_owner_setup", { password });
}

/** Verifies `password` and, on success, grants an elevated session. */
export function login(password: string): Promise<void> {
  return call<void>("product_owner_login", { password });
}

/** Ends the elevated session immediately. */
export function logout(): Promise<void> {
  return call<void>("product_owner_logout", {});
}

/** Every module's state, including per-platform lock flags. Requires a
 * currently-valid elevated session (see `login`). */
export function getModules(platform: Platform): Promise<ModuleState[]> {
  return call<ModuleState[]>("product_owner_get_modules", { platform });
}

/** Sets `enabled` and/or `locked` for one module on one platform,
 * independently — pass `null` for whichever half should stay unchanged. */
export function setModule(
  moduleKey: string,
  platform: Platform,
  enabled: boolean | null,
  locked: boolean | null,
): Promise<ModuleState[]> {
  return call<ModuleState[]>("product_owner_set_module", { moduleKey, platform, enabled, locked });
}
