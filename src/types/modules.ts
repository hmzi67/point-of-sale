/**
 * The fixed catalogue of modules the product can ship. Which of these are
 * visible for a given installation is configuration, not code — the enabled
 * set is read from SQLite and the sidebar/routes derive from it.
 *
 * Settings is deliberately NOT a module: it is the screen that configures the
 * modules, so it is always present for owner/admin.
 */
export type ModuleKey =
  | "dashboard"
  | "billing"
  | "inventory"
  | "reports"
  | "tables"
  | "attendance"
  | "expenses"
  | "salary";

export type Platform = "desktop" | "android";

/** This build's platform. Tauri Mobile (Phase 12) flips this to "android". */
export const PLATFORM: Platform = "desktop";

/** One row of the module catalogue joined with its stored visibility. */
export interface ModuleState {
  id: number;
  key: ModuleKey;
  name: string;
  /** Core modules (billing) can never be toggled off. */
  isCore: boolean;
  sortOrder: number;
  /** Visibility on the platform the list was requested for. */
  enabled: boolean;
  desktopEnabled: boolean;
  androidEnabled: boolean;
}
