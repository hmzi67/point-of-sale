import type { SyncResult, SyncStatusInfo } from "../types";
import { call, subscribe } from "./tauriClient";

/**
 * Retrieves the current synchronization metadata and status,
 * including unsynced transaction count and server customer ID.
 */
export function getSyncStatus(): Promise<SyncStatusInfo> {
  return call<SyncStatusInfo>("pos_get_sync_status");
}

/**
 * Triggers an immediate cloud synchronization with diwan.codehunts.co.uk.
 */
export function syncNow(): Promise<SyncResult> {
  return call<SyncResult>("pos_sync_now");
}

/**
 * Subscribes to sync completion events emitted from background or manual sync rounds.
 */
export function onSyncCompleted(handler: (result: SyncResult) => void) {
  return subscribe<SyncResult>("pos:sync_completed", handler);
}
