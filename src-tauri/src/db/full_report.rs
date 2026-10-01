//! The "Generate Full Report" consolidated document: one aggregate over
//! everything the individual per-report exports (`reports::*`) show
//! separately — the Overview summary (including net profit, reusing
//! `dashboard::get_summary` rather than recomputing that math a second
//! time), itemized Refunds, Category Wise Sale, Product Wise Sales, and
//! Table Wise Sales (only when the `tables` module is enabled) — all for
//! the same date range, assembled once here so the downloadable PDF and the
//! thermal-print version are built from the exact same numbers rather than
//! two separate queries that could theoretically disagree.

use rusqlite::Connection;
use serde::Serialize;

use crate::db::dashboard::{self, DashboardSummary};
use crate::db::modules::{self, Platform};
use crate::db::reports::{
    self, CategorySalesReport, ProductSalesSummaryReport, RefundsSummary, ReportError, SalesSummary,
    TableSalesSummary, TopItemSort,
};

#[derive(Debug)]
pub enum FullReportError {
    Dashboard(dashboard::DashboardError),
    Report(ReportError),
    Sqlite(rusqlite::Error),
}

impl std::fmt::Display for FullReportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FullReportError::Dashboard(err) => write!(f, "{}", err),
            FullReportError::Report(err) => write!(f, "{}", err),
            FullReportError::Sqlite(err) => write!(f, "database error: {}", err),
        }
    }
}

impl From<dashboard::DashboardError> for FullReportError {
    fn from(err: dashboard::DashboardError) -> Self {
        FullReportError::Dashboard(err)
    }
}

impl From<ReportError> for FullReportError {
    fn from(err: ReportError) -> Self {
        FullReportError::Report(err)
    }
}

impl From<rusqlite::Error> for FullReportError {
    fn from(err: rusqlite::Error) -> Self {
        FullReportError::Sqlite(err)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FullReport {
    pub start_date: String,
    pub end_date: String,
    /// Total sales, refunds, transaction count and net profit — the exact
    /// same struct (and math) the Dashboard screen uses, so a figure on
    /// this report can never quietly disagree with the Dashboard for the
    /// same range.
    pub overview: DashboardSummary,
    /// `average_sale_minor` specifically — `DashboardSummary` doesn't carry
    /// it (Dashboard's own cards don't show it), but the Reports Overview
    /// tab does, so this reuses `reports::get_sales_summary`'s rounding
    /// rather than recomputing an average a second, different way.
    pub average_sale_minor: i64,
    /// Itemized refunds for the period, most recent first, plus a grand
    /// total — never module-gated (same as `overview.refunds_minor`), so
    /// this is always present, unlike `table_sales` below.
    pub refunds: RefundsSummary,
    pub category_sales: CategorySalesReport,
    pub product_sales: ProductSalesSummaryReport,
    /// `None` when the `tables` module is disabled for this
    /// installation/platform — same "not tracked, not a zero" convention
    /// `DashboardSummary` uses for expenses/salary/inventory, rather than
    /// shipping an empty/meaningless table section.
    pub table_sales: Option<TableSalesSummary>,
}

/// Assembles one [`FullReport`] for `start_date`..`end_date` (inclusive),
/// scoped to whichever optional modules `platform` currently has enabled —
/// same module-gating contract `dashboard::get_summary` follows.
pub fn get_full_report(
    conn: &Connection,
    start_date: &str,
    end_date: &str,
    platform: Platform,
) -> Result<FullReport, FullReportError> {
    let overview = dashboard::get_summary(conn, start_date, end_date, platform)?;
    let SalesSummary { average_sale_minor, .. } = reports::get_sales_summary(conn, start_date, end_date)?;
    let refunds = reports::get_refunds_summary(conn, start_date, end_date)?;
    let category_sales = reports::get_category_sales(conn, start_date, end_date)?;
    // Product Wise Sales isn't module-gated (unlike Table Wise Sales below)
    // — every installation has products to sell, ranked by revenue to
    // match the report's on-screen default sort.
    let product_sales = reports::get_product_sales_summary(conn, start_date, end_date, None, TopItemSort::Revenue)?;

    let tables_enabled = modules::list(conn, platform)?.into_iter().any(|m| m.key == "tables" && m.enabled);
    let table_sales =
        if tables_enabled { Some(reports::get_table_sales_summary(conn, start_date, end_date)?) } else { None };

    Ok(FullReport {
        start_date: start_date.to_string(),
        end_date: end_date.to_string(),
        overview,
        average_sale_minor,
        refunds,
        category_sales,
        product_sales,
        table_sales,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::test_conn;

    /// The seeded sales fall within the last few weeks; reports cap how
    /// wide a range may be, so use a recent window rather than "all time".
    fn range() -> (String, String) {
        let today = chrono::Local::now().date_naive();
        let start = today - chrono::Duration::days(60);
        (start.format("%Y-%m-%d").to_string(), today.format("%Y-%m-%d").to_string())
    }

    #[test]
    fn table_sales_follow_the_tables_module() {
        let conn = test_conn();
        let platform = Platform::Desktop;
        let (start, end) = range();
        let (start, end) = (start.as_str(), end.as_str());

        modules::set_enabled(&conn, "tables", platform, true).unwrap();
        assert!(get_full_report(&conn, start, end, platform).unwrap().table_sales.is_some());

        modules::set_enabled(&conn, "tables", platform, false).unwrap();
        assert!(get_full_report(&conn, start, end, platform).unwrap().table_sales.is_none());
    }

    /// The whole point of assembling one payload: every section agrees with
    /// the standalone query it stands in for.
    #[test]
    fn sections_match_the_standalone_reports_for_the_same_range() {
        let conn = test_conn();
        let platform = Platform::Desktop;
        let (start, end) = range();
        let (start, end) = (start.as_str(), end.as_str());
        let report = get_full_report(&conn, start, end, platform).unwrap();

        let dashboard = dashboard::get_summary(&conn, start, end, platform).unwrap();
        assert_eq!(report.overview.total_sales_minor, dashboard.total_sales_minor);
        assert_eq!(report.overview.refunds_minor, dashboard.refunds_minor);
        assert_eq!(report.overview.transaction_count, dashboard.transaction_count);

        let summary = reports::get_sales_summary(&conn, start, end).unwrap();
        assert_eq!(report.average_sale_minor, summary.average_sale_minor);
        assert_eq!(report.overview.total_sales_minor, summary.total_sales_minor);

        let refunds = reports::get_refunds_summary(&conn, start, end).unwrap();
        assert_eq!(report.refunds.grand_total_refunded_minor, refunds.grand_total_refunded_minor);
        let categories = reports::get_category_sales(&conn, start, end).unwrap();
        assert_eq!(report.category_sales.grand_total_minor, categories.grand_total_minor);
    }

    #[test]
    fn a_backwards_range_is_rejected() {
        let conn = test_conn();
        let (start, end) = range();
        assert!(get_full_report(&conn, &end, &start, Platform::Desktop).is_err());
    }
}
