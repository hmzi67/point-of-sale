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
    /// Set once the first-time setup wizard finishes. `false` is what tells
    /// the frontend to route a freshly-installed client into onboarding
    /// instead of the normal app.
    pub onboarding_completed: bool,
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
    pub onboarding_completed: Option<bool>,
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
        onboarding_completed: row.get::<_, i64>("onboarding_completed")? != 0,
    })
}

pub fn get(conn: &Connection) -> Result<AppConfig, rusqlite::Error> {
    conn.query_row(
        "SELECT business_name, business_type, logo_path, currency, tax_percent, receipt_footer,
                working_days_per_month, onboarding_completed
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
                working_days_per_month = COALESCE(?7, working_days_per_month),
                onboarding_completed = COALESCE(?8, onboarding_completed)
          WHERE id = 1",
        params![
            patch.business_name,
            patch.business_type,
            patch.logo_path,
            patch.currency,
            patch.tax_percent,
            patch.receipt_footer,
            patch.working_days_per_month,
            patch.onboarding_completed,
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
