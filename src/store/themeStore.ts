import { create } from "zustand";
import { getTheme, setTheme as persistTheme } from "../services/configService";
import type { Theme } from "../types";

/**
 * The UI colour theme. The DOM is the thing actually being themed — every
 * colour utility resolves through `:root[data-theme=…]` in `styles.css` —
 * so this store's one real job is to keep `<html data-theme>` in sync with
 * the persisted preference and to expose the current value for the toggle.
 *
 * Persistence is `app_config.theme` in SQLite (the same place every other
 * per-install preference lives — no localStorage, nothing new), read via
 * `config_get_theme` at boot *before* login so the login screen is themed
 * too. Dark is the default: `index.html` ships `data-theme="dark"`
 * statically, so a dark install never repaints and a light one repaints
 * once, as soon as the read resolves.
 */
interface ThemeState {
  theme: Theme;
  /** Reads the persisted theme and applies it. Safe to call before login;
   * a failed read leaves the default in place rather than surfacing. */
  init: () => Promise<void>;
  /** Applies immediately (the flip must feel instant), then persists. A
   * failed persist is logged, not reverted — the user asked for this theme
   * and got it; it just won't survive a restart. */
  setTheme: (theme: Theme) => Promise<void>;
  toggle: () => Promise<void>;
}

function applyToDocument(theme: Theme) {
  document.documentElement.dataset.theme = theme;
}

export const useThemeStore = create<ThemeState>((set, get) => ({
  theme: "dark",

  init: async () => {
    try {
      const theme = await getTheme();
      applyToDocument(theme);
      set({ theme });
    } catch (error) {
      console.warn("[theme] could not read persisted theme, keeping default", error);
    }
  },

  setTheme: async (theme) => {
    applyToDocument(theme);
    set({ theme });
    try {
      await persistTheme(theme);
    } catch (error) {
      console.warn("[theme] could not persist theme", error);
    }
  },

  toggle: () => get().setTheme(get().theme === "dark" ? "light" : "dark"),
}));
