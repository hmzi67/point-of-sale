import type { AppConfig, Theme, UiPrefs } from "../types";
import { call } from "./tauriClient";

export function getAppConfig(): Promise<AppConfig> {
  return call<AppConfig>("config_get");
}

/** Partial update — omitted fields keep their stored value. */
export function updateAppConfig(patch: Partial<AppConfig>): Promise<AppConfig> {
  return call<AppConfig>("config_update", { patch });
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

/** The persisted display preferences (theme, sidebar). Readable before
 * login — the login and lock screens are themed too — and never a reason
 * to block boot: callers fall back to the defaults if this fails. */
export function getUiPrefs(): Promise<UiPrefs> {
  return call<UiPrefs>("config_get_ui_prefs");
}

/** Persists the desktop sidebar's collapsed state (any signed-in user). */
export function setSidebarCollapsed(collapsed: boolean): Promise<void> {
  return call<void>("config_set_sidebar_collapsed", { collapsed });
}

/** Persists the theme for this install (any signed-in user — it's a
 * display preference for the till's own screen, not a business setting). */
export function setTheme(theme: Theme): Promise<void> {
  return call<void>("config_set_theme", { theme });
}
