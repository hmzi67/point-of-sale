import type { WorkspaceConnectOutcome, WorkspaceInfo } from "../types";
import { call, subscribe } from "./tauriClient";

/** This counter's place in its business workspace (see `src-tauri/src/workspace/`). */
export function getWorkspaceInfo(): Promise<WorkspaceInfo> {
  return call<WorkspaceInfo>("workspace_get_info");
}

/** Registers this device as a counter of its licence's business. */
export function connectWorkspace(counterName: string | null): Promise<WorkspaceConnectOutcome> {
  return call<WorkspaceConnectOutcome>("workspace_connect", { counterName });
}

/** The business is empty: this counter's data becomes the business data. */
export function uploadToWorkspace(): Promise<WorkspaceInfo> {
  return call<WorkspaceInfo>("workspace_upload");
}

/** Replaces this counter's data with the business data; resolves to the backup's path. */
export function downloadFromWorkspace(): Promise<string> {
  return call<string>("workspace_download");
}

export function disconnectWorkspace(): Promise<void> {
  return call<void>("workspace_disconnect");
}

export function renameCounter(name: string): Promise<WorkspaceInfo> {
  return call<WorkspaceInfo>("workspace_rename_counter", { name });
}

export function syncWorkspaceNow(): Promise<WorkspaceInfo> {
  return call<WorkspaceInfo>("workspace_sync_now");
}

export function dismissWorkspaceIssue(id: number): Promise<WorkspaceInfo> {
  return call<WorkspaceInfo>("workspace_dismiss_issue", { id });
}

export function onWorkspaceStatus(handler: (info: WorkspaceInfo) => void) {
  return subscribe<WorkspaceInfo>("workspace:status", handler);
}

/** Records from other counters landed: screens showing them should reload. */
export function onWorkspaceApplied(handler: () => void) {
  return subscribe<null>("workspace:applied", handler);
}

/** This counter now holds the business data (and its staff): sign out. */
export function onWorkspaceReplaced(handler: () => void) {
  return subscribe<null>("workspace:replaced", handler);
}
