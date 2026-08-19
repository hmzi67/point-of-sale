import type { ModuleState, Platform } from "../types";
import { call } from "./tauriClient";

/** Every module with its visibility for `platform` (not just the enabled ones). */
export function getEnabledModules(platform: Platform): Promise<ModuleState[]> {
  return call<ModuleState[]>("get_enabled_modules", { platform });
}

/** Toggles one module on one platform and returns the refreshed list. */
export function toggleModule(
  moduleKey: string,
  platform: Platform,
  enabled: boolean,
): Promise<ModuleState[]> {
  return call<ModuleState[]>("toggle_module", { moduleKey, platform, enabled });
}
