/** Mirrors `workspace::runtime::WorkspaceInfo` (Rust). */
export type WorkspaceMode = "standalone" | "pending" | "connected";

/** What the top bar shows; `setup` covers "registered, not finished". */
export type WorkspaceSyncState = "synced" | "pending" | "offline" | "error" | "seeding" | "standalone" | "setup";

export interface WorkspaceCounter {
  code: string;
  name: string;
  active: boolean;
  lastSeenAt: string | null;
  thisCounter: boolean;
}

export interface WorkspaceIssue {
  /** 0 for a record still waiting to be applied (not dismissable). */
  id: number;
  kind: string;
  detail: string;
  createdAt: string;
}

export interface WorkspaceInfo {
  mode: WorkspaceMode;
  state: WorkspaceSyncState;
  businessName: string | null;
  counterName: string | null;
  counterCode: string | null;
  /** Changes on this counter the business has not received yet. */
  pending: number;
  /** Downloaded records not applied yet (see `issues`). */
  parked: number;
  lastOkAt: string | null;
  lastError: string | null;
  seeding: boolean;
  counters: WorkspaceCounter[];
  issues: WorkspaceIssue[];
}

/** What to do after connecting, from `workspace_connect`. */
export interface WorkspaceConnectOutcome {
  next: "upload" | "download" | "wait" | "connected";
  businessName: string;
  counterName: string;
  counters: number;
  maxCounters: number;
  plan: string | null;
  /** Records here that `download` would replace (after a backup). */
  localRecords: number;
}
