import { create } from "zustand";
import {
  activateLicense,
  getLicenseInfo,
  getLicensePlan,
  getLicenseState,
  onLicenseChanged,
  refreshLicense,
} from "../services/licenseService";
import { useModuleStore } from "./moduleStore";
import type { LicenseInfo, LicensePlan, LicenseState } from "../types";

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
  /** The plan and its entitlements; null until first read. */
  plan: LicensePlan | null;
  isBusy: boolean;
  error: string | null;
  /** Reads the state once and subscribes to changes. Idempotent. */
  init: () => Promise<void>;
  loadInfo: () => Promise<void>;
  /** Re-reads the plan, and the module list whose `entitled` flags follow it. */
  loadPlan: () => Promise<void>;
  /** "Re-check now" — resolves to a hint string when offline, else null. */
  refresh: () => Promise<string | null>;
  activate: (licenseKey: string) => Promise<void>;
}

let unsubscribe: (() => void) | null = null;

export const useLicenseStore = create<LicenseStoreState>((set, get) => ({
  state: null,
  info: null,
  plan: null,
  isBusy: false,
  error: null,

  init: async () => {
    if (unsubscribe === null) {
      unsubscribe = await onLicenseChanged((state) => {
        set({ state });
        // The masked key / last-check time, and the plan, can change with the state.
        void get().loadInfo();
        void get().loadPlan();
      });
    }
    try {
      set({ state: await getLicenseState() });
      void get().loadPlan();
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

  loadPlan: async () => {
    try {
      const plan = await getLicensePlan();
      const before = get().plan;
      set({ plan });
      // Module entitlements are part of the module list: reload it when the
      // plan actually changed (not on the first read, which App does itself).
      if (before && JSON.stringify(before) !== JSON.stringify(plan) && useModuleStore.getState().isLoaded) {
        void useModuleStore.getState().load();
      }
    } catch {
      // Presentation only: commands still enforce the plan.
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
