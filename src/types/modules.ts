import { platform as osPlatform } from "@tauri-apps/plugin-os";

/**
 * The fixed catalogue of modules the product can ship. Which of these are
 * visible for a given installation is configuration, not code — the enabled
 * set is read from SQLite and the sidebar/routes derive from it.
 *
 * Settings is deliberately NOT a module: it is the screen that configures the
 * modules, so it is always present for owner/admin.
 */
export type ModuleKey =
  | "dashboard"
  | "billing"
  | "inventory"
  | "reports"
  | "tables"
  | "attendance"
  | "expenses"
  | "salary"
  | "shifts";

export type Platform = "desktop" | "android";

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

/** One row of the module catalogue joined with its stored visibility. */
export interface ModuleState {
  id: number;
  key: ModuleKey;
  name: string;
  /** Core modules (billing) can never be toggled off. */
  isCore: boolean;
  sortOrder: number;
  /** Visibility on the platform the list was requested for. */
  enabled: boolean;
  desktopEnabled: boolean;
  androidEnabled: boolean;
  /** Whether the product owner has locked *this platform's* visibility —
   * when true, the client's own Owner/Admin cannot change `enabled` here;
   * only the hidden product-owner override can. Derived from the requested
   * platform's lock column, same as `enabled` is. */
  locked: boolean;
  desktopLocked: boolean;
  androidLocked: boolean;
}
