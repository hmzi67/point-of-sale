import { create } from "zustand";
import { getEnabledModules, toggleModule } from "../services/moduleService";
import { PLATFORM, type ModuleKey, type ModuleState } from "../types";

/**
 * The module configuration for this installation. Everything that decides what
 * a client can see — sidebar, routes, dashboard cards — reads from here, so a
 * toggle written to SQLite is reflected across the UI on the next render with
 * no restart and no code change.
 */
interface ModuleStoreState {
  modules: ModuleState[];
  isLoaded: boolean;
  isLoading: boolean;
  error: string | null;
  load: () => Promise<void>;
  toggle: (key: ModuleKey, enabled: boolean) => Promise<void>;
}

export const useModuleStore = create<ModuleStoreState>((set) => ({
  modules: [],
  isLoaded: false,
  isLoading: false,
  error: null,

  load: async () => {
    set({ isLoading: true, error: null });
    try {
      const modules = await getEnabledModules(PLATFORM);
      set({ modules, isLoaded: true, isLoading: false });
    } catch (error) {
      set({ error: (error as Error).message, isLoading: false, isLoaded: true });
    }
  },

  toggle: async (key, enabled) => {
    set({ error: null });
    try {
      // The command returns the refreshed list, so one round trip keeps the
      // store in sync with what SQLite actually stored.
      const modules = await toggleModule(key, PLATFORM, enabled);
      set({ modules });
    } catch (error) {
      set({ error: (error as Error).message });
      throw error;
    }
  },
}));
