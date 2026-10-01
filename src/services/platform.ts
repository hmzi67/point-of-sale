import { platform as osPlatform } from "@tauri-apps/plugin-os";
import type { Platform } from "../types";

/* Platform detection lives here, not in `types/`, because it calls a Tauri
 * plugin — and plugin access goes through `services/`, the same boundary
 * as every `invoke()`. */

/**
 * This build's platform, detected at runtime via `@tauri-apps/plugin-os`
 * (its `platform()` reads a value baked in at build/webview-init time, so
 * this is a synchronous, no-IPC call — safe to evaluate once at module load
 * rather than needing to be async). Everything that isn't Android — every
 * desktop OS the Tauri build targets — collapses to `"desktop"` here; this
 * product only ever distinguishes "the Android build" from "everything
 * else", never individual desktop OSes.
 *
 * Guarded because `platform()` reads `window.__TAURI_OS_PLUGIN_INTERNALS__`,
 * which the plugin only injects inside an actual Tauri webview — `npm run
 * dev`'s plain-browser mode (see CLAUDE.md) has no such global, and must
 * still fall back to "desktop" rather than throwing at module load.
 */
function detectPlatform(): Platform {
  try {
    return osPlatform() === "android" ? "android" : "desktop";
  } catch {
    return "desktop";
  }
}

export const PLATFORM: Platform = detectPlatform();

/** Convenience for layout branching (bottom tab bar vs. the desktop
 * hamburger overlay) — equivalent to `PLATFORM === "android"`. */
export const IS_ANDROID = PLATFORM === "android";

/**
 * Whether this desktop build is running on Windows specifically —
 * `PLATFORM`/`IS_ANDROID` deliberately collapse every desktop OS to one
 * bucket, but the printer transport genuinely differs between them (see
 * `printer::windows_spool`'s doc comment on the Rust side: Windows needs
 * the Print Spooler and an explicit by-name printer selection, unlike
 * macOS/Linux's USB auto-detect), so `PrinterSettingsSection` needs to tell
 * Windows apart from the rest of desktop specifically. Same try/catch
 * fallback as `detectPlatform` for the same reason — `npm run dev`'s plain
 * browser has no `@tauri-apps/plugin-os` global to read.
 */
export const IS_WINDOWS: boolean = (() => {
  try {
    return osPlatform() === "windows";
  } catch {
    return false;
  }
})();
