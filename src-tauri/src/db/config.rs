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
    /// Divisor for salary calculation — see `db/salary.rs`.
    pub working_days_per_month: i64,
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
    pub working_days_per_month: Option<i64>,
}

fn from_row(row: &Row<'_>) -> Result<AppConfig, rusqlite::Error> {
    Ok(AppConfig {
        business_name: row.get("business_name")?,
        business_type: row.get("business_type")?,
        logo_path: row.get("logo_path")?,
        currency: row.get("currency")?,
        tax_percent: row.get("tax_percent")?,
        receipt_footer: row.get("receipt_footer")?,
        working_days_per_month: row.get("working_days_per_month")?,
    })
}

pub fn get(conn: &Connection) -> Result<AppConfig, rusqlite::Error> {
    conn.query_row(
        "SELECT business_name, business_type, logo_path, currency, tax_percent, receipt_footer,
                working_days_per_month
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
                working_days_per_month = COALESCE(?7, working_days_per_month)
          WHERE id = 1",
        params![
            patch.business_name,
            patch.business_type,
            patch.logo_path,
            patch.currency,
            patch.tax_percent,
            patch.receipt_footer,
            patch.working_days_per_month,
        ],
    )?;

    get(conn)
}
