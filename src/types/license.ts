/**
 * The licence state as Rust computed it — see `src-tauri/src/license/state.rs`
 * for the meaning of each variant and the checks behind them. The frontend
 * only ever *renders* this; it never recomputes expiry or reads the payload
 * itself. Every write that matters (a sale, a refund, a kitchen token,
 * finishing setup) is refused in Rust while `kind !== "ok"`, so nothing
 * here is load-bearing for enforcement.
 */
export type LicenseState =
  | { kind: "missing" }
  | { kind: "ok"; expiresAt: number | null; expiringSoon: boolean }
  | { kind: "expired"; expiredAt: number | null }
  | { kind: "revoked" }
  | { kind: "suspended" }
  | { kind: "deviceMismatch" }
  | { kind: "clockTampered"; reason: "behindServerTime" | "behindLastSeenTime" }
  | { kind: "checkInRequired"; lastOnlineCheck: number; maxOfflineDays: number }
  | { kind: "invalid"; message: string };

/**
 * The licence's plan and what it unlocks, from the signed payload (Rust's
 * `license::entitlements::Snapshot`). `restricted: false` = a licence issued
 * before plans existed (or none yet): every module stays available.
 * Presentation only — every command re-checks in Rust.
 */
export interface LicensePlan {
  restricted: boolean;
  planCode: string | null;
  planName: string | null;
  /** Module and capability keys the plan unlocks (empty when unrestricted). */
  entitlements: string[];
  /** key → the cheapest plan that includes it, for keys this plan lacks. */
  upgrade: Record<string, string>;
}

/** Features checked inside a module (see `entitlements::capability`). */
export type Capability = "inventory_advanced" | "reports_advanced" | "multi_user" | "mobile";

export interface LicenseInfo {
  /** `DIWAN-XXXX-••••-••••-••••`, or null when no key is stored. */
  keyPrefix: string | null;
  /** Unix seconds of the last successful server refresh. */
  lastOnlineCheck: number | null;
  /** Where the app is talking to — shown in Settings for support calls. */
  apiBase: string;
}

export interface LicenseRefreshOutcome {
  state: LicenseState;
  /** Set when the server couldn't be reached; `state` is then the cached one. */
  hint: string | null;
}
