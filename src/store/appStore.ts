import { create } from "zustand";
import { getAppConfig, updateAppConfig } from "../services/configService";
import type { AppConfig } from "../types";

/** Installation config (business identity, currency, tax), backed by `app_config`. */
interface AppState {
  config: AppConfig;
  isLoadingConfig: boolean;
  error: string | null;
  load: () => Promise<void>;
  save: (patch: Partial<AppConfig>) => Promise<void>;
}

/** Shown until the real config arrives from SQLite. */
const placeholderConfig: AppConfig = {
  businessName: "…",
  businessType: "retail",
  logoPath: null,
  currency: "PKR",
  taxPercent: 0,
  receiptFooter: "",
};

export const useAppStore = create<AppState>((set) => ({
  config: placeholderConfig,
  isLoadingConfig: false,
  error: null,

  load: async () => {
    set({ isLoadingConfig: true, error: null });
    try {
      const config = await getAppConfig();
      set({ config, isLoadingConfig: false });
    } catch (error) {
      set({ error: (error as Error).message, isLoadingConfig: false });
    }
  },

  save: async (patch) => {
    set({ error: null });
    try {
      const config = await updateAppConfig(patch);
      set({ config });
    } catch (error) {
      set({ error: (error as Error).message });
      throw error;
    }
  },
}));
