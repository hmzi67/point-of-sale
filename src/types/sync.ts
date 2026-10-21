export interface SyncStatusInfo {
  lastSyncedSaleId: number;
  lastSyncedAt: string | null;
  serverCustomerId: number | null;
  totalSalesCount: number;
  unsyncedCount: number;
  isLicenseActive: boolean;
  licensePrefix: string | null;
  serverUrl: string;
}

export interface SyncResult {
  success: boolean;
  syncedSales: number;
  syncedSummaries: number;
  serverCustomerId: number | null;
  message: string;
}
