import type { LicenseInfo, LicenseRefreshOutcome, LicenseState } from "../types";
import { call, subscribe } from "./tauriClient";

export function getLicenseState(): Promise<LicenseState> {
  return call<LicenseState>("license_get_state");
}

export function getLicenseInfo(): Promise<LicenseInfo> {
  return call<LicenseInfo>("license_get_info");
}

/** "Re-check now". Resolves even when offline (see `hint`); rejects only on a
 * server refusal or a response that fails verification. */
export function refreshLicense(): Promise<LicenseRefreshOutcome> {
  return call<LicenseRefreshOutcome>("license_refresh");
}

/** Activates `licenseKey` for this device and stores its signed status.
 * Owner/Admin only; needs the network. */
export function activateLicense(licenseKey: string): Promise<LicenseState> {
  return call<LicenseState>("license_activate", { licenseKey });
}

/** Fires whenever Rust's licence state changes — background refresh, the
 * per-minute re-evaluation, an activation from another screen. */
export function onLicenseChanged(handler: (state: LicenseState) => void) {
  return subscribe<LicenseState>("license:changed", handler);
}
