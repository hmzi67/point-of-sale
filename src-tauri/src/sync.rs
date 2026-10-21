//! Cloud synchronization with diwan.codehunts.co.uk.
//!
//! Synchronizes offline-first sales and daily performance summaries with the
//! user's cloud account on diwan.codehunts.co.uk based on their licence key and
//! device fingerprint. Ensures sales records and dashboards reflect live shop data.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::db::sync::{SyncResult, SyncSalePayload, SyncStatusInfo};
use crate::db::Db;
use crate::license::runtime::LicenseRuntime;

pub const SYNC_COMPLETED_EVENT: &str = "pos:sync_completed";

/// Guard against concurrent sync calls.
static IS_SYNCING: AtomicBool = AtomicBool::new(false);

/// Response payload from POST /api/pos-sync.php.
#[derive(Debug, Deserialize)]
struct ServerSyncResponse {
    #[allow(dead_code)]
    status: String,
    customer_id: i64,
    synced_sales: usize,
    synced_summaries: usize,
    #[allow(dead_code)]
    server_time: Option<String>,
}

/// Retrieves the current sync tracking status.
pub fn get_status(app: &AppHandle) -> Result<SyncStatusInfo, String> {
    let db = app.state::<Db>();
    let api_url = crate::license::client::api_base();
    db.with_conn(|conn| crate::db::sync::get_sync_status(conn, &api_url))
        .map_err(|e| e.to_string())
}

/// Executes a synchronization round with the remote cloud server.
/// Gathers new sales since the last sync high-water mark, as well as recent
/// daily performance summaries, and pushes them to diwan.codehunts.co.uk.
pub async fn perform_sync(app: &AppHandle) -> Result<SyncResult, String> {
    if IS_SYNCING.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        return Ok(SyncResult {
            success: false,
            synced_sales: 0,
            synced_summaries: 0,
            server_customer_id: None,
            message: "Synchronization is already in progress.".into(),
        });
    }

    let result = perform_sync_inner(app).await;
    IS_SYNCING.store(false, Ordering::SeqCst);

    if let Ok(ref outcome) = result {
        let _ = app.emit(SYNC_COMPLETED_EVENT, outcome);
    }

    result
}

async fn perform_sync_inner(app: &AppHandle) -> Result<SyncResult, String> {
    let db = app.state::<Db>();
    let license = app.state::<LicenseRuntime>();

    // 1. Device fingerprint
    let fingerprint = license
        .device_fingerprint()
        .map_err(|e| format!("Could not identify device fingerprint: {e}"))?;

    // 2. Active license key
    let license_key = db
        .with_conn(|conn| {
            crate::db::license::get(conn).map(|rec| rec.map(|r| r.license_key))
        })
        .map_err(|e| e.to_string())?
        .unwrap_or_default();

    if license_key.trim().is_empty() {
        return Ok(SyncResult {
            success: false,
            synced_sales: 0,
            synced_summaries: 0,
            server_customer_id: None,
            message: "No active licence key found. Activate Diwan to sync with your dashboard.".into(),
        });
    }

    // 3. Shop settings (currency, machine hint)
    let (currency, shop_name): (String, String) = db
        .with_conn(|conn| {
            let config = crate::db::config::get(conn)?;
            Ok::<_, rusqlite::Error>((config.currency, config.business_name))
        })
        .unwrap_or_else(|_| ("PKR".to_string(), "POS Terminal".to_string()));

    let api_base = crate::license::client::api_base();
    let status_info = db
        .with_conn(|conn| crate::db::sync::get_sync_status(conn, &api_base))
        .map_err(|e| e.to_string())?;

    // 4. Gather unsynced sales and recent daily summaries
    let sales = db
        .with_conn(|conn| {
            crate::db::sync::get_sales_for_sync(conn, status_info.last_synced_sale_id, 100, &currency)
        })
        .map_err(|e| e.to_string())?;

    let daily_summaries = db
        .with_conn(|conn| crate::db::sync::get_recent_daily_summaries(conn, 14, &currency))
        .map_err(|e| e.to_string())?;

    let records = db
        .with_conn(crate::db::sync::get_records_for_sync)
        .map_err(|e| e.to_string())?;

    let machine_hint = machine_hint_for(&shop_name, &crate::license::client::machine_hint());

    // 5. Build JSON payload
    let payload = serde_json::json!({
        "license_key": license_key,
        "machine_fingerprint": fingerprint,
        "machine_hint": machine_hint,
        "customer_id": status_info.server_customer_id,
        "sales": sales,
        "daily_summaries": daily_summaries,
        "records": records,
    });

    // 6. Post to pos-sync.php
    let response: ServerSyncResponse = match crate::license::client::post_with_timeout(
        "pos-sync.php",
        &payload,
        Duration::from_secs(30),
    )
    .await {
        Ok(res) => res,
        Err(crate::license::client::ClientError::Offline(msg)) => {
            return Ok(SyncResult {
                success: false,
                synced_sales: 0,
                synced_summaries: 0,
                server_customer_id: status_info.server_customer_id,
                message: format!("Device is offline: {msg}"),
            });
        }
        Err(e) => return Err(format!("Cloud server error: {e}")),
    };

    // 7. On success, record high-water mark and customer ID in local SQLite
    let max_sale_id = high_water_mark(&sales, status_info.last_synced_sale_id);

    db.with_conn(|conn| {
        crate::db::sync::record_sync_success(conn, max_sale_id, Some(response.customer_id))
    })
    .map_err(|e| e.to_string())?;

    crate::startup_log::log(&format!(
        "pos sync: successfully synced {} sales, {} summaries",
        response.synced_sales, response.synced_summaries
    ));

    Ok(SyncResult {
        success: true,
        synced_sales: response.synced_sales,
        synced_summaries: response.synced_summaries,
        server_customer_id: Some(response.customer_id),
        message: format!(
            "Successfully synced {} sales and {} daily records with customer dashboard #{}.",
            response.synced_sales, response.synced_summaries, response.customer_id
        ),
    })
}

/// The sale id to resume from next time: the newest sale just sent, never
/// below where we already were (an empty batch keeps the old mark).
fn high_water_mark(sales: &[SyncSalePayload], previous: i64) -> i64 {
    sales.iter().map(|s| s.local_sale_id).fold(previous, i64::max)
}

/// The device label the dashboard shows: "Shop (host)" or just the host.
fn machine_hint_for(shop_name: &str, host_hint: &str) -> String {
    let shop = shop_name.trim();
    if shop.is_empty() { host_hint.to_string() } else { format!("{shop} ({host_hint})") }
}

/// Spawns a periodic background sync worker.
/// First sync runs 8 seconds after app start, then every 15 minutes.
pub fn spawn_background_sync(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(8)).await;
        let mut interval = tokio::time::interval(Duration::from_secs(15 * 60));
        loop {
            interval.tick().await;
            match perform_sync(&app).await {
                Ok(res) => {
                    if res.success && (res.synced_sales > 0 || res.synced_summaries > 0) {
                        crate::startup_log::log(&format!(
                            "pos background sync: {} sales, {} summaries",
                            res.synced_sales, res.synced_summaries
                        ));
                    }
                }
                Err(err) => {
                    crate::startup_log::warn(&format!("pos background sync error: {}", crate::startup_log::clip(&err.to_string())));
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sale(local_sale_id: i64) -> SyncSalePayload {
        SyncSalePayload {
            local_sale_id,
            order_no: None,
            table_name: None,
            cashier_name: None,
            total_minor: 100,
            discount_minor: 0,
            tax_minor: 0,
            payment_method: "cash".into(),
            currency: "PKR".into(),
            items_count: 1,
            items_summary: "Tea x1".into(),
            sale_created_at: "2031-03-04 10:00:00".into(),
        }
    }

    #[test]
    fn high_water_mark_takes_the_newest_sent_sale_and_never_goes_back() {
        assert_eq!(high_water_mark(&[sale(7), sale(12), sale(9)], 6), 12);
        assert_eq!(high_water_mark(&[], 6), 6, "an empty batch keeps the old mark");
        assert_eq!(high_water_mark(&[sale(3)], 6), 6);
    }

    #[test]
    fn machine_hint_prefixes_the_shop_name_only_when_set() {
        assert_eq!(machine_hint_for("  Cafe One ", "mac-mini"), "Cafe One (mac-mini)");
        assert_eq!(machine_hint_for("   ", "mac-mini"), "mac-mini");
    }
}
