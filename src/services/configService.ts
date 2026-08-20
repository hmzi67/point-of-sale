import type { AppConfig } from "../types";
import { call } from "./tauriClient";

export function getAppConfig(): Promise<AppConfig> {
  return call<AppConfig>("get_app_config");
}

/** Partial update — omitted fields keep their stored value. */
export function updateAppConfig(patch: Partial<AppConfig>): Promise<AppConfig> {
  return call<AppConfig>("update_app_config", { patch });
}

/** Uploads a logo image, replacing whatever is currently set, and returns
 * the refreshed config (already pointing `logoPath` at the new file). */
export function uploadLogo(dataBase64: string, extension: string): Promise<AppConfig> {
  return call<AppConfig>("config_upload_logo", { dataBase64, extension });
}

/** Reads the current logo back as a `data:` URL for direct use as an
 * `<img src>`. */
export function getLogoDataUrl(fileName: string): Promise<string> {
  return call<string>("config_get_logo", { fileName });
}
