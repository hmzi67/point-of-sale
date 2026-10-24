import { create } from "zustand";
import {
  getWorkspaceInfo,
  onWorkspaceApplied,
  onWorkspaceReplaced,
  onWorkspaceStatus,
  syncWorkspaceNow,
} from "../services/workspaceService";
import { useAppStore } from "./appStore";
import { useAuthStore } from "./authStore";
import { useInventoryStore } from "./inventoryStore";
import { useModuleStore } from "./moduleStore";
import type { WorkspaceInfo } from "../types";

/** Window event for screens that keep their own data: records from other counters arrived. */
export const WORKSPACE_DATA_CHANGED = "diwan:workspace-data-changed";

/**
 * This counter's place in its business workspace, for the top bar and
 * Settings. Kept current by Rust's `workspace:status` event (sent after
 * every sync round), so nothing polls. When another counter's changes land
 * (`workspace:applied`) the shared stores reload and screens with their own
 * state hear `WORKSPACE_DATA_CHANGED`; when this counter's data is replaced
 * by the business data (`workspace:replaced`) everyone is signed out — the
 * staff accounts are now the business's.
 */
interface WorkspaceStoreState {
  info: WorkspaceInfo | null;
  isSyncing: boolean;
  /** Reads the info once and subscribes to changes. Idempotent. */
  init: () => Promise<void>;
  refresh: () => Promise<void>;
  syncNow: () => Promise<void>;
  setInfo: (info: WorkspaceInfo) => void;
}

let subscribed = false;

function reloadSharedData() {
  void useAppStore.getState().load();
  void useModuleStore.getState().load();
  if (useInventoryStore.getState().isLoaded) void useInventoryStore.getState().load();
  useInventoryStore.getState().retryMissingImages();
  window.dispatchEvent(new Event(WORKSPACE_DATA_CHANGED));
}

export const useWorkspaceStore = create<WorkspaceStoreState>((set, get) => ({
  info: null,
  isSyncing: false,

  init: async () => {
    if (!subscribed) {
      subscribed = true;
      await onWorkspaceStatus((info) => set({ info }));
      await onWorkspaceApplied(reloadSharedData);
      await onWorkspaceReplaced(() => {
        useAuthStore.getState().logout();
        reloadSharedData();
      });
    }
    await get().refresh();
  },

  refresh: async () => {
    try {
      set({ info: await getWorkspaceInfo() });
    } catch {
      // Signed out, or not in Tauri: the chip simply isn't shown.
    }
  },

  syncNow: async () => {
    set({ isSyncing: true });
    try {
      set({ info: await syncWorkspaceNow() });
    } finally {
      set({ isSyncing: false });
    }
  },

  setInfo: (info) => set({ info }),
}));
