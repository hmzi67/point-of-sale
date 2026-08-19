import type { AppConfig } from "../types";
import { call } from "./tauriClient";

export function getAppConfig(): Promise<AppConfig> {
  return call<AppConfig>("get_app_config");
}

/** Partial update — omitted fields keep their stored value. */
export function updateAppConfig(patch: Partial<AppConfig>): Promise<AppConfig> {
  return call<AppConfig>("update_app_config", { patch });
}
