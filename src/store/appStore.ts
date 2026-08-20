import { create } from "zustand";
import { getAppConfig, updateAppConfig, uploadLogo } from "../services/configService";
import type { AppConfig } from "../types";

/** Installation config (business identity, currency, tax), backed by `app_config`. */
interface AppState {
  config: AppConfig;
  isLoadingConfig: boolean;
  error: string | null;
  load: () => Promise<void>;
  save: (patch: Partial<AppConfig>) => Promise<void>;
  /** Uploads a new logo and updates `config.logoPath` to it — every
   * subscriber (TopBar, Settings' preview) re-renders with the new image
   * the moment this resolves, no restart needed, same as `save`. */
  uploadLogo: (dataBase64: string, extension: string) => Promise<void>;
}

/** Shown until the real config arrives from SQLite. */
const placeholderConfig: AppConfig = {
  businessName: "…",
  businessType: "retail",
  logoPath: null,
  currency: "PKR",
  taxPercent: 0,
  receiptFooter: "",
  workingDaysPerMonth: 26,
  // A conservative placeholder: `true` (not the real, usually-`false` value)
  // so the onboarding-gate check below never fires a redirect off a value
  // that hasn't loaded from SQLite yet — it waits on `isLoadingConfig`
  // instead. See `RequireOnboarding`.
  onboardingCompleted: true,
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

  uploadLogo: async (dataBase64, extension) => {
    set({ error: null });
    try {
      const config = await uploadLogo(dataBase64, extension);
      set({ config });
    } catch (error) {
      set({ error: (error as Error).message });
      throw error;
    }
  },
}));
