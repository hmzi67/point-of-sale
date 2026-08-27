//! Reads and writes for the single-row `app_config` table.

use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub business_name: String,
    pub business_type: String,
    pub logo_path: Option<String>,
    pub currency: String,
    pub tax_percent: f64,
    pub receipt_footer: String,
    /// Business contact number, shown in Settings and printed on receipts.
    /// `None` until an owner/admin sets one.
    pub phone: Option<String>,
    /// Delivery/dispatch contact number, shown in Settings and printed on
    /// receipts (labeled "Delivery: ...") only when set. `None` until an
    /// owner/admin sets one — a shop with no delivery service just never
    /// shows the line.
    pub delivery_number: Option<String>,
    /// Set once the first-time setup wizard finishes. `false` is what tells
    /// the frontend to route a freshly-installed client into onboarding
    /// instead of the normal app.
    pub onboarding_completed: bool,
    /// The chosen printer transport, set from Settings' "Select printer"
    /// step (see `commands::printer_*` and `printer::escpos::send_to_printer`
    /// for how this is used) — `None` until a cashier/owner has actually
    /// gone through that step once, which is deliberately the same as "no
    /// printer": printing must never guess at a transport. `"usb"`
    /// (macOS/Linux, informational — USB is auto-detected either way),
    /// `"bluetooth"` (Android), or `"windows"` (an installed printer chosen
    /// by name, sent to via the Print Spooler — see
    /// `printer::windows_spool`).
    pub printer_connection_type: Option<String>,
    /// The paired device's MAC address (Android Bluetooth only). Present
    /// only when `printer_connection_type == Some("bluetooth")`.
    pub printer_bluetooth_address: Option<String>,
    /// The paired device's display name, stored alongside the address
    /// purely so Settings can show "Selected: <name>" without needing a
    /// live Bluetooth query just to render the current selection.
    pub printer_bluetooth_name: Option<String>,
    /// The selected Windows printer's name, exactly as `winspool` reports
    /// it (e.g. `"POS-80 Thermal Printer"`) — this *is* the address on
    /// Windows, unlike Bluetooth's separate address/name pair, since
    /// `OpenPrinterW` opens printers by this same name. Present only when
    /// `printer_connection_type == Some("windows")`.
    pub printer_windows_name: Option<String>,
}

/// One paired-device entry in Settings' "Select printer" list — a plain
/// serializable DTO for `commands::printer_list_bluetooth_devices`, kept
/// separate from `printer::android_bt::BluetoothDeviceInfo` (which has no
/// reason to derive `Serialize` — it's internal to that platform-gated
/// module) so this one type can be referenced from cross-platform command
/// signatures without pulling `android_bt` into non-Android builds.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BluetoothDeviceOption {
    pub name: String,
    pub address: String,
}

/// One installed printer in Settings' Windows "Select printer" list — a
/// plain serializable DTO for `commands::printer_list_windows_printers`,
/// kept separate from `printer::windows_spool::WindowsPrinterInfo` for the
/// same reason `BluetoothDeviceOption` is kept separate from
/// `android_bt::BluetoothDeviceInfo`: this one can be referenced from
/// cross-platform command signatures without pulling the Windows-only
/// module into non-Windows builds. No separate address field — unlike
/// Bluetooth, a Windows printer's name *is* what `OpenPrinterW` opens it
/// by, so there's nothing else to store.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowsPrinterOption {
    pub name: String,
}

/// Fields a caller may change. `None` means "leave as-is", so the frontend can
/// send a partial update without re-sending the whole config.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfigUpdate {
    pub business_name: Option<String>,
    pub business_type: Option<String>,
    pub logo_path: Option<String>,
    pub currency: Option<String>,
    pub tax_percent: Option<f64>,
    pub receipt_footer: Option<String>,
    pub phone: Option<String>,
    pub delivery_number: Option<String>,
    pub onboarding_completed: Option<bool>,
    pub printer_connection_type: Option<String>,
    pub printer_bluetooth_address: Option<String>,
    pub printer_bluetooth_name: Option<String>,
    pub printer_windows_name: Option<String>,
}

fn from_row(row: &Row<'_>) -> Result<AppConfig, rusqlite::Error> {
    Ok(AppConfig {
        business_name: row.get("business_name")?,
        business_type: row.get("business_type")?,
        logo_path: row.get("logo_path")?,
        currency: row.get("currency")?,
        tax_percent: row.get("tax_percent")?,
        receipt_footer: row.get("receipt_footer")?,
        phone: row.get("phone")?,
        delivery_number: row.get("delivery_number")?,
        onboarding_completed: row.get::<_, i64>("onboarding_completed")? != 0,
        printer_connection_type: row.get("printer_connection_type")?,
        printer_bluetooth_address: row.get("printer_bluetooth_address")?,
        printer_bluetooth_name: row.get("printer_bluetooth_name")?,
        printer_windows_name: row.get("printer_windows_name")?,
    })
}

pub fn get(conn: &Connection) -> Result<AppConfig, rusqlite::Error> {
    conn.query_row(
        "SELECT business_name, business_type, logo_path, currency, tax_percent, receipt_footer,
                phone, delivery_number, onboarding_completed,
                printer_connection_type, printer_bluetooth_address, printer_bluetooth_name,
                printer_windows_name
           FROM app_config WHERE id = 1",
        [],
        from_row,
    )
}

/// Applies a partial update and returns the stored config afterwards.
/// `COALESCE(?, column)` keeps every omitted field at its current value.
pub fn update(conn: &Connection, patch: AppConfigUpdate) -> Result<AppConfig, rusqlite::Error> {
    conn.execute(
        "UPDATE app_config
            SET business_name  = COALESCE(?1, business_name),
                business_type  = COALESCE(?2, business_type),
                logo_path      = COALESCE(?3, logo_path),
                currency       = COALESCE(?4, currency),
                tax_percent    = COALESCE(?5, tax_percent),
                receipt_footer = COALESCE(?6, receipt_footer),
                phone          = COALESCE(?7, phone),
                delivery_number = COALESCE(?8, delivery_number),
                onboarding_completed = COALESCE(?9, onboarding_completed),
                printer_connection_type = COALESCE(?10, printer_connection_type),
                printer_bluetooth_address = COALESCE(?11, printer_bluetooth_address),
                printer_bluetooth_name = COALESCE(?12, printer_bluetooth_name),
                printer_windows_name = COALESCE(?13, printer_windows_name)
          WHERE id = 1",
        params![
            patch.business_name,
            patch.business_type,
            patch.logo_path,
            patch.currency,
            patch.tax_percent,
            patch.receipt_footer,
            patch.phone,
            patch.delivery_number,
            patch.onboarding_completed,
            patch.printer_connection_type,
            patch.printer_bluetooth_address,
            patch.printer_bluetooth_name,
            patch.printer_windows_name,
        ],
    )?;

    get(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::test_conn;

    #[test]
    fn a_fresh_install_has_not_completed_onboarding() {
        let conn = test_conn();
        assert!(!get(&conn).unwrap().onboarding_completed, "a brand-new database must route through setup");
    }

    #[test]
    fn update_can_mark_onboarding_complete_without_touching_other_fields() {
        let conn = test_conn();
        let before = get(&conn).unwrap();

        let after = update(
            &conn,
            AppConfigUpdate { onboarding_completed: Some(true), ..Default::default() },
        )
        .unwrap();

        assert!(after.onboarding_completed);
        assert_eq!(after.business_name, before.business_name, "omitted fields must be left as-is");
        assert_eq!(after.tax_percent, before.tax_percent);
    }

    #[test]
    fn omitting_onboarding_completed_leaves_it_unchanged() {
        let conn = test_conn();
        update(&conn, AppConfigUpdate { onboarding_completed: Some(true), ..Default::default() }).unwrap();

        let after = update(&conn, AppConfigUpdate { business_name: Some("New Name".into()), ..Default::default() })
            .unwrap();
        assert!(after.onboarding_completed, "a patch that doesn't mention the flag must not reset it");
    }
}
