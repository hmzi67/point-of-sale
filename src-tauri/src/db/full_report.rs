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
