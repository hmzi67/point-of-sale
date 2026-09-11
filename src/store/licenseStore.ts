import { create } from "zustand";
import {
  activateLicense,
  getLicenseInfo,
  getLicenseState,
  onLicenseChanged,
  refreshLicense,
  type LicenseInfo,
  type LicenseState,
} from "../services/licenseService";

/**
 * Mirror of Rust's licence state for rendering: the warning banner, the
 * lock screen and the settings section all read `state` from here, and it
 * is kept current by the `license:changed` event so none of them poll.
 *
 * `state === null` means "not loaded yet" — distinct from `{kind: "missing"}`
 * — so guards don't flash the lock screen during the first read.
 */
interface LicenseStoreState {
  state: LicenseState | null;
  info: LicenseInfo | null;
  isBusy: boolean;
  error: string | null;
  /** Reads the state once and subscribes to changes. Idempotent. */
  init: () => Promise<void>;
  loadInfo: () => Promise<void>;
  /** "Re-check now" — resolves to a hint string when offline, else null. */
  refresh: () => Promise<string | null>;
  activate: (licenseKey: string) => Promise<void>;
}

let unsubscribe: (() => void) | null = null;

export const useLicenseStore = create<LicenseStoreState>((set, get) => ({
  state: null,
  info: null,
  isBusy: false,
  error: null,

  init: async () => {
    if (unsubscribe === null) {
      unsubscribe = await onLicenseChanged((state) => {
        set({ state });
        // The masked key / last-check time can change with the state.
        void get().loadInfo();
      });
    }
    try {
      set({ state: await getLicenseState() });
    } catch (error) {
      // Can't read the state at all: treat as locked with the reason, the
      // same way Rust reports its own failures.
      set({ state: { kind: "invalid", message: (error as Error).message } });
    }
  },

  loadInfo: async () => {
    try {
      set({ info: await getLicenseInfo() });
    } catch {
      // Purely informational; leave whatever was there.
    }
  },

  refresh: async () => {
    set({ isBusy: true, error: null });
    try {
      const outcome = await refreshLicense();
      set({ state: outcome.state, isBusy: false });
      void get().loadInfo();
      return outcome.hint;
    } catch (error) {
      set({ error: (error as Error).message, isBusy: false });
      throw error;
    }
  },

  activate: async (licenseKey) => {
    set({ isBusy: true, error: null });
    try {
      const state = await activateLicense(licenseKey);
      set({ state, isBusy: false });
      void get().loadInfo();
    } catch (error) {
      set({ error: (error as Error).message, isBusy: false });
      throw error;
    }
  },
}));
