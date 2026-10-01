/**
 * The fixed catalogue of modules the product can ship. Which of these are
 * visible for a given installation is configuration, not code — the enabled
 * set is read from SQLite and the sidebar/routes derive from it.
 *
 * Every sidebar section is vendor-configurable, including administration.
 */
export type ModuleKey =
  | "dashboard"
  | "billing"
  | "inventory"
  | "reports"
  | "tables"
  | "attendance"
  | "expenses"
  | "salary"
  | "shifts"
  | "employees"
  | "users"
  | "settings";

export type Platform = "desktop" | "android";

/** One row of the module catalogue joined with its stored visibility. */
export interface ModuleState {
  id: number;
  key: ModuleKey;
  name: string;
  /** Core modules (billing) can only be disabled by the vendor. */
  isCore: boolean;
  sortOrder: number;
  /** Visibility on the platform the list was requested for. */
  enabled: boolean;
  desktopEnabled: boolean;
  androidEnabled: boolean;
  /** Whether the product owner has locked *this platform's* visibility —
   * when true, the client's own Owner/Admin cannot change `enabled` here;
   * only the hidden product-owner override can. Derived from the requested
   * platform's lock column, same as `enabled` is. */
  locked: boolean;
  desktopLocked: boolean;
  androidLocked: boolean;
}
