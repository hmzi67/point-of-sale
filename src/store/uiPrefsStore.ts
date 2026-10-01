import { create } from "zustand";
import { getUiPrefs, setSidebarCollapsed as persistSidebar, setTheme as persistTheme } from "../services/configService";
import { devWarn, reportError } from "../services/logService";
import type { Theme } from "../types";

/**
 * Display preferences for this device: colour theme and the desktop
 * sidebar's collapsed state. Both persist to `app_config` in SQLite (the
 * same place every other per-install preference lives — no localStorage,
 * nothing new) and are read in one round-trip at boot, *before* login, so
 * the login screen is themed too.
 *
 * Theme: the DOM is the thing actually being themed — every colour utility
 * resolves through `:root[data-theme=…]` in `styles.css` — so the store's
 * real job there is keeping `<html data-theme>` in sync. Dark is the
 * default: `index.html` ships `data-theme="dark"` statically, so a dark
 * install never repaints and a light one repaints once.
 *
 * Every setter applies immediately (the flip must feel instant) and then
 * persists; a failed persist is logged, not reverted — the user asked for
 * it and got it, it just won't survive a restart.
 */
interface UiPrefsState {
  theme: Theme;
  sidebarCollapsed: boolean;
  /** Reads the persisted preferences and applies them. Safe to call before
   * login; a failed read leaves the defaults in place rather than surfacing. */
  init: () => Promise<void>;
  setTheme: (theme: Theme) => Promise<void>;
  toggleTheme: () => Promise<void>;
  setSidebarCollapsed: (collapsed: boolean) => Promise<void>;
  toggleSidebar: () => Promise<void>;
}

function applyThemeToDocument(theme: Theme) {
  document.documentElement.dataset.theme = theme;
}

export const useUiPrefsStore = create<UiPrefsState>((set, get) => ({
  theme: "dark",
  sidebarCollapsed: false,

  init: async () => {
    try {
      const prefs = await getUiPrefs();
      applyThemeToDocument(prefs.theme);
      set({ theme: prefs.theme, sidebarCollapsed: prefs.sidebarCollapsed });
    } catch (error) {
      devWarn("[uiPrefs] could not read persisted preferences, keeping defaults", error);
    }
  },

  setTheme: async (theme) => {
    // Optimistic, but a failed write rolls the toggle back so what the user
    // sees always matches what was saved — the visible snap-back is how the
    // failure surfaces, and the persistent log records why.
    const previous = get().theme;
    applyThemeToDocument(theme);
    set({ theme });
    try {
      await persistTheme(theme);
    } catch (error) {
      applyThemeToDocument(previous);
      set({ theme: previous });
      reportError(`[uiPrefs] could not persist theme: ${(error as Error).message}`);
    }
  },

  toggleTheme: () => get().setTheme(get().theme === "dark" ? "light" : "dark"),

  setSidebarCollapsed: async (collapsed) => {
    const previous = get().sidebarCollapsed;
    set({ sidebarCollapsed: collapsed });
    try {
      await persistSidebar(collapsed);
    } catch (error) {
      set({ sidebarCollapsed: previous });
      reportError(`[uiPrefs] could not persist sidebar state: ${(error as Error).message}`);
    }
  },

  toggleSidebar: () => get().setSidebarCollapsed(!get().sidebarCollapsed),
}));
