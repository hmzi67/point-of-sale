import type { DeliveryPlatform, RecentRider, DeliveryReport } from "../types";
import { call } from "./tauriClient";

/** Lists all delivery platforms (both active and inactive) for Settings. */
export function listPlatforms(): Promise<DeliveryPlatform[]> {
  return call<DeliveryPlatform[]>("delivery_platforms_list", {});
}

/** Lists only active delivery platforms for the billing screen dropdown. */
export function listActivePlatforms(): Promise<DeliveryPlatform[]> {
  return call<DeliveryPlatform[]>("delivery_platforms_list_active", {});
}

/** Adds a new delivery platform. */
export function addPlatform(name: string): Promise<DeliveryPlatform> {
  return call<DeliveryPlatform>("delivery_platforms_add", { name });
}

/** Renames an existing delivery platform. */
export function updatePlatform(id: number, name: string): Promise<DeliveryPlatform> {
  return call<DeliveryPlatform>("delivery_platforms_update", { id, name });
}

/** Activates or deactivates a delivery platform. */
export function setPlatformActive(id: number, isActive: boolean): Promise<void> {
  return call<void>("delivery_platforms_set_active", { id, isActive });
}

/** Fetches distinct recent riders (name + phone) used for a given platform. */
export function getRecentRiders(platformId: number, limit = 10): Promise<RecentRider[]> {
  return call<RecentRider[]>("delivery_platforms_get_recent_riders", { platformId, limit });
}

/** Fetches the delivery report for a date range. */
export function getDeliveryReport(startDate: string, endDate: string): Promise<DeliveryReport> {
  return call<DeliveryReport>("reports_get_delivery_report", { startDate, endDate });
}
