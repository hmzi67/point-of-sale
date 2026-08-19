import { PLATFORM } from "../types";
import type { DashboardSummary } from "../types";
import { call } from "./tauriClient";

/** Sales, expenses and salary payouts for `startDate`..`endDate` (inclusive),
 * scoped to whichever optional modules are enabled on this platform. A
 * disabled module's figure comes back `null`, not `0` — see `dashboard.rs`. */
export function getDashboardSummary(startDate: string, endDate: string): Promise<DashboardSummary> {
  return call<DashboardSummary>("dashboard_get_summary", { startDate, endDate, platform: PLATFORM });
}
