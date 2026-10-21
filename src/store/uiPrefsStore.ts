import { create } from "zustand";
import { getUiPrefs, setSidebarCollapsed as persistSidebar, setTheme as persistTheme } from "../services/configService";
import { devWarn, reportWarning } from "../services/logService";
import { IS_ANDROID } from "../services/platform";
import type { GlassEffect, Theme } from "../types";

/**
 * Display preferences for this device: colour theme, desktop sidebar
 * collapsed state, and glass effect.
 */
interface UiPrefsState {
  theme: Theme;
  sidebarCollapsed: boolean;
  glassEffect: GlassEffect;
  /** Reads the persisted preferences and applies them. Safe to call before
   * login; a failed read leaves the defaults in place rather than surfacing. */
  init: () => Promise<void>;
  setTheme: (theme: Theme) => Promise<void>;
  toggleTheme: () => Promise<void>;
  setSidebarCollapsed: (collapsed: boolean) => Promise<void>;
  toggleSidebar: () => Promise<void>;
  setGlassEffect: (glassEffect: GlassEffect) => void;
}

function detectDefaultGlass(): GlassEffect {
  if (IS_ANDROID) return "off";
  if (typeof window !== "undefined") {
    try {
      if (window.matchMedia?.("(prefers-reduced-motion: reduce)").matches) {
        return "off";
      }
      const nav = navigator as any;
      if (nav.hardwareConcurrency && nav.hardwareConcurrency <= 2) {
        return "off";
      }
      if (nav.deviceMemory && nav.deviceMemory <= 2) {
        return "off";
      }
    } catch {
      // ignore
    }
  }
  return "subtle";
}

function getInitialGlassEffect(): GlassEffect {
  if (typeof window !== "undefined") {
    try {
      const stored = localStorage.getItem("pos_glass_effect");
      if (stored === "off" || stored === "subtle") {
        return stored;
      }
    } catch {
      // ignore
    }
  }
  return detectDefaultGlass();
}

function applyThemeToDocument(theme: Theme) {
  if (typeof document !== "undefined") {
    document.documentElement.dataset.theme = theme;
  }
}

function applyGlassToDocument(glass: GlassEffect) {
  if (typeof document !== "undefined") {
    document.documentElement.dataset.glass = glass;
  }
}

// Initialise root attribute immediately on module load
applyGlassToDocument(getInitialGlassEffect());

export const useUiPrefsStore = create<UiPrefsState>((set, get) => ({
  theme: "dark",
  sidebarCollapsed: false,
  glassEffect: getInitialGlassEffect(),

  init: async () => {
    try {
      const glass = getInitialGlassEffect();
      applyGlassToDocument(glass);
      const prefs = await getUiPrefs();
      applyThemeToDocument(prefs.theme);
      set({ theme: prefs.theme, sidebarCollapsed: prefs.sidebarCollapsed, glassEffect: glass });
    } catch (error) {
      devWarn("[uiPrefs] could not read persisted preferences, keeping defaults", error);
    }
  },

  setGlassEffect: (glassEffect: GlassEffect) => {
    applyGlassToDocument(glassEffect);
    try {
      localStorage.setItem("pos_glass_effect", glassEffect);
    } catch {
      // ignore
    }
    set({ glassEffect });
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
      reportWarning(`[uiPrefs] could not persist theme: ${(error as Error).message}`);
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
      reportWarning(`[uiPrefs] could not persist sidebar state: ${(error as Error).message}`);
    }
  },

  toggleSidebar: () => get().setSidebarCollapsed(!get().sidebarCollapsed),
}));
