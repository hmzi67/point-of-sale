//! The owner/admin dashboard: one aggregate over sales, expenses and salary
//! payouts for a date range, plus a low-stock count.
//!
//! Every one of those source tables (`sales`, `expenses`, `salary_payments`,
//! `items`) always exists regardless of which modules a client has enabled —
//! disabling a module only hides its screen, it never deletes data. So a
//! client who disabled Expenses after using it for a while still has
//! `expenses` rows sitting in SQLite. The dashboard must not fold those into
//! "net profit" once the client has told the product that module isn't part
//! of their business — the whole point of Phase 1's module system is that a
//! disabled module is *off*, not "off but secretly still counted". So this
//! module checks `enabled_modules` for `expenses`/`salary`/`inventory` before
//! including each in the response at all: an unset field, not a zero one,
//! is how the frontend tells "no expense tracking here" apart from "tracked
//! and it happened to be zero this period".

use rusqlite::{params, Connection};
use serde::Serialize;

use crate::db::modules::{self, Platform};
use crate::db::reports::{self, ReportError};

#[derive(Debug)]
pub enum DashboardError {
    Report(ReportError),
    Sqlite(rusqlite::Error),
}

impl std::fmt::Display for DashboardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DashboardError::Report(err) => write!(f, "{}", err),
            DashboardError::Sqlite(err) => write!(f, "database error: {}", err),
        }
    }
}

impl From<rusqlite::Error> for DashboardError {
    fn from(err: rusqlite::Error) -> Self {
        DashboardError::Sqlite(err)
    }
}

impl From<ReportError> for DashboardError {
    fn from(err: ReportError) -> Self {
        DashboardError::Report(err)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardSummary {
    pub start_date: String,
    pub end_date: String,

    /// Gross — see `reports::SalesSummary::total_sales_minor`.
    pub total_sales_minor: i64,
    /// Refunds recorded in the range — always counted (refunds aren't a
    /// module, unlike expenses/salary/inventory below), so this is never
    /// `None`, only possibly `0`.
    pub refunds_minor: i64,
    pub transaction_count: i64,

    /// `None` when the `expenses` module is disabled for this
    /// installation/platform — not counted toward `net_profit_minor` either.
    pub total_expenses_minor: Option<i64>,
    /// `None` when the `salary` module is disabled — same treatment.
    pub total_salary_paid_minor: Option<i64>,
    /// `(sales - refunds) - (expenses if tracked) - (salary paid if
    /// tracked)` — refunds are never optional-module-gated the way
    /// expenses/salary are, so they always reduce this.
    pub net_profit_minor: i64,

    /// `None` when the `inventory` module is disabled.
    pub low_stock_item_count: Option<i64>,

    /// `None` when the `tables` module is disabled, *or* when it's enabled
    /// but no table-attributed sale happened in the range — "no table sold
    /// anything" has nothing meaningful to show either way, same spirit as
    /// `low_stock_item_count` only rendering client-side when non-zero.
    pub top_table_by_sales: Option<TopTable>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopTable {
    pub table_id: i64,
    pub name: String,
    pub total_minor: i64,
}

fn module_enabled(conn: &Connection, platform: Platform, key: &str) -> Result<bool, rusqlite::Error> {
    Ok(modules::list(conn, platform)?.into_iter().any(|m| m.key == key && m.enabled))
}

fn expenses_total(conn: &Connection, start_date: &str, end_date: &str) -> Result<i64, rusqlite::Error> {
    conn.query_row(
        "SELECT COALESCE(SUM(amount_minor), 0) FROM expenses WHERE expense_date BETWEEN ?1 AND ?2",
        params![start_date, end_date],
        |row| row.get(0),
    )
}

fn salary_paid_total(conn: &Connection, start_date: &str, end_date: &str) -> Result<i64, rusqlite::Error> {
    // `paid_date` is the day money actually went out — the same "which day
    // does this belong to" convention `expense_date` uses, and NULL for a
    // month that's been calculated but never paid, so those are excluded
    // by the range comparison rather than needing an explicit IS NOT NULL.
    conn.query_row(
        "SELECT COALESCE(SUM(paid_amount_minor), 0) FROM salary_payments WHERE paid_date BETWEEN ?1 AND ?2",
        params![start_date, end_date],
        |row| row.get(0),
    )
}

fn low_stock_count(conn: &Connection) -> Result<i64, rusqlite::Error> {
    conn.query_row(
        "SELECT COUNT(*) FROM items WHERE is_active = 1 AND stock_qty <= low_stock_threshold",
        [],
        |row| row.get(0),
    )
}

/// The single highest-selling real table in the range, if any — reuses
/// `reports::get_table_sales_summary` (already sorted highest-first, already
/// reconciled against the overall total) rather than a parallel query, and
/// just takes its first row that actually has a `table_id` (skipping the
/// "Counter / Takeaway" row, which isn't a table).
fn top_table_by_sales(conn: &Connection, start_date: &str, end_date: &str) -> Result<Option<TopTable>, DashboardError> {
    let table_sales = reports::get_table_sales_summary(conn, start_date, end_date)?;
    Ok(table_sales
        .rows
        .into_iter()
        .find_map(|row| row.table_id.map(|table_id| TopTable { table_id, name: row.label, total_minor: row.total_minor })))
}

/// Aggregates sales, expenses and salary payouts for `start_date`..`end_date`
/// (inclusive), scoped to whichever optional modules `platform` currently has
/// enabled. Billing is core and always contributes; Expenses, Salary and
/// Inventory contribute only when enabled, and come back as `None` — not
/// `Some(0)` — when they're off, so the frontend can tell "not tracked" apart
/// from "tracked, zero this period" and skip rendering that card.
pub fn get_summary(
    conn: &Connection,
    start_date: &str,
    end_date: &str,
    platform: Platform,
) -> Result<DashboardSummary, DashboardError> {
    let sales = reports::get_sales_summary(conn, start_date, end_date)?;

    let total_expenses_minor = if module_enabled(conn, platform, "expenses")? {
        Some(expenses_total(conn, start_date, end_date)?)
    } else {
        None
    };
    let total_salary_paid_minor = if module_enabled(conn, platform, "salary")? {
        Some(salary_paid_total(conn, start_date, end_date)?)
    } else {
        None
    };
    let low_stock_item_count = if module_enabled(conn, platform, "inventory")? {
        Some(low_stock_count(conn)?)
    } else {
        None
    };
    let top_table_by_sales = if module_enabled(conn, platform, "tables")? {
        top_table_by_sales(conn, start_date, end_date)?
    } else {
        None
    };

    let net_profit_minor =
        sales.net_sales_minor - total_expenses_minor.unwrap_or(0) - total_salary_paid_minor.unwrap_or(0);

    Ok(DashboardSummary {
        start_date: start_date.to_string(),
        end_date: end_date.to_string(),
        total_sales_minor: sales.total_sales_minor,
        refunds_minor: sales.refunds_minor,
        transaction_count: sales.transaction_count,
        total_expenses_minor,
        total_salary_paid_minor,
        net_profit_minor,
        low_stock_item_count,
        top_table_by_sales,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::test_conn;

    /// A range wide enough to cover all of the seed data (sales/expenses
    /// within the last ~12 days, last month's salary payment) but inside
    /// `reports::MAX_RANGE_DAYS`.
    fn wide_range() -> (String, String) {
        let today = chrono::Local::now().date_naive();
        let start = today - chrono::Duration::days(60);
        (start.format("%Y-%m-%d").to_string(), today.format("%Y-%m-%d").to_string())
    }

    #[test]
    fn summary_includes_expenses_and_salary_when_both_modules_are_enabled() {
        let conn = test_conn();
        let (start, end) = wide_range();
        let summary = get_summary(&conn, &start, &end, Platform::Desktop).unwrap();

        assert!(summary.total_sales_minor > 0, "seed data has sales");
        assert!(summary.total_expenses_minor.is_some());
        assert!(summary.total_salary_paid_minor.is_some());
        assert!(summary.low_stock_item_count.is_some());

        let expected_profit = summary.total_sales_minor
            - summary.total_expenses_minor.unwrap()
            - summary.total_salary_paid_minor.unwrap();
        assert_eq!(summary.net_profit_minor, expected_profit);
    }

    #[test]
    fn disabling_expenses_drops_it_from_the_summary_and_the_profit_math() {
        let conn = test_conn();
        conn.execute(
            "UPDATE enabled_modules SET desktop_enabled = 0
              WHERE module_id = (SELECT id FROM modules WHERE key = 'expenses')",
            [],
        )
        .unwrap();

        let (start, end) = wide_range();
        let summary = get_summary(&conn, &start, &end, Platform::Desktop).unwrap();
        assert!(summary.total_expenses_minor.is_none(), "disabled module must be None, not Some(0)");
        assert!(summary.total_salary_paid_minor.is_some(), "salary is untouched and stays enabled");

        // Profit no longer subtracts expenses at all, even though the shop's
        // old expense rows are still sitting in the database.
        let expected_profit = summary.total_sales_minor - summary.total_salary_paid_minor.unwrap();
        assert_eq!(summary.net_profit_minor, expected_profit);
    }

    #[test]
    fn disabling_every_optional_module_still_produces_a_correct_summary() {
        let conn = test_conn();
        for key in ["expenses", "salary", "inventory", "tables", "attendance"] {
            conn.execute(
                "UPDATE enabled_modules SET desktop_enabled = 0
                  WHERE module_id = (SELECT id FROM modules WHERE key = ?1)",
                params![key],
            )
            .unwrap();
        }

        let (start, end) = wide_range();
        let summary = get_summary(&conn, &start, &end, Platform::Desktop).unwrap();
        assert!(summary.total_expenses_minor.is_none());
        assert!(summary.total_salary_paid_minor.is_none());
        assert!(summary.low_stock_item_count.is_none());
        // Billing can never be disabled, so sales still come through untouched.
        assert_eq!(summary.net_profit_minor, summary.total_sales_minor);
    }

    #[test]
    fn platforms_are_independent_disabling_on_android_leaves_desktop_untouched() {
        let conn = test_conn();
        conn.execute(
            "UPDATE enabled_modules SET android_enabled = 0
              WHERE module_id = (SELECT id FROM modules WHERE key = 'expenses')",
            [],
        )
        .unwrap();

        let (start, end) = wide_range();
        let desktop = get_summary(&conn, &start, &end, Platform::Desktop).unwrap();
        assert!(desktop.total_expenses_minor.is_some());

        let android = get_summary(&conn, &start, &end, Platform::Android).unwrap();
        assert!(android.total_expenses_minor.is_none());
    }

    #[test]
    fn rejects_a_backwards_date_range() {
        let conn = test_conn();
        assert!(matches!(
            get_summary(&conn, "2026-02-01", "2026-01-01", Platform::Desktop),
            Err(DashboardError::Report(_))
        ));
    }

    /// The whole point of wiring refunds into reports: a refund must not be
    /// an isolated ledger entry nobody else's totals notice.
    #[test]
    fn a_refund_reduces_net_profit_by_exactly_its_amount() {
        use crate::db::refunds::{create_refund, CreateRefundInput, RefundLineInput};
        use crate::db::sales::{create_sale, CartLine, CreateSaleInput};
        use chrono::Local;

        let mut conn = test_conn();
        let today = Local::now().format("%Y-%m-%d").to_string();

        let (sale_id, sale_item_id) = {
            let tx = conn.transaction().unwrap();
            let cola: i64 =
                tx.query_row("SELECT id FROM items WHERE name = 'Cola 500ml'", [], |row| row.get(0)).unwrap();
            let sale = create_sale(
                &tx,
                CreateSaleInput {
                    items: vec![CartLine { item_id: cola, qty: 1.0, notes: None }],
                    discount_minor: 0,
                    tax_minor: 0,
                    payment_method: "cash".into(),
                    cashier_id: None,
                    table_id: None,
                    shift_id: None,
                },
            )
            .unwrap();
            let sale_item_id: i64 = tx
                .query_row("SELECT id FROM sale_items WHERE sale_id = ?1", params![sale.id], |row| row.get(0))
                .unwrap();
            tx.commit().unwrap();
            (sale.id, sale_item_id)
        };

        let before = get_summary(&conn, &today, &today, Platform::Desktop).unwrap();

        {
            let tx = conn.transaction().unwrap();
            create_refund(
                &tx,
                CreateRefundInput {
                    sale_id,
                    items: vec![RefundLineInput { sale_item_id, qty: 1.0, amount_minor: 8000 }],
                    reason: None,
                    refunded_by: None,
                },
            )
            .unwrap();
            tx.commit().unwrap();
        }

        let after = get_summary(&conn, &today, &today, Platform::Desktop).unwrap();
        assert_eq!(after.total_sales_minor, before.total_sales_minor, "gross sales unaffected");
        assert_eq!(after.refunds_minor, before.refunds_minor + 8000);
        assert_eq!(
            after.net_profit_minor,
            before.net_profit_minor - 8000,
            "net profit must drop by exactly the refunded amount"
        );
    }

    #[test]
    fn top_table_is_none_when_tables_module_is_disabled() {
        let conn = test_conn();
        conn.execute(
            "UPDATE enabled_modules SET desktop_enabled = 0
              WHERE module_id = (SELECT id FROM modules WHERE key = 'tables')",
            [],
        )
        .unwrap();

        let (start, end) = wide_range();
        let summary = get_summary(&conn, &start, &end, Platform::Desktop).unwrap();
        assert!(summary.top_table_by_sales.is_none());
    }

    #[test]
    fn top_table_is_none_when_enabled_but_nothing_was_sold_to_a_table() {
        let conn = test_conn(); // seed data has no table-linked sales
        let (start, end) = wide_range();
        let summary = get_summary(&conn, &start, &end, Platform::Desktop).unwrap();
        assert!(summary.top_table_by_sales.is_none(), "no table sold anything — nothing to highlight");
    }

    #[test]
    fn top_table_reflects_the_highest_selling_real_table() {
        use crate::db::sales::{create_sale, CartLine, CreateSaleInput};
        use crate::db::tables::start_table_order;

        let mut conn = test_conn();
        let tx = conn.transaction().unwrap();
        let table_id: i64 = tx.query_row("SELECT id FROM tables LIMIT 1", [], |row| row.get(0)).unwrap();
        let table_name: String =
            tx.query_row("SELECT name FROM tables WHERE id = ?1", params![table_id], |row| row.get(0)).unwrap();
        start_table_order(&tx, table_id).unwrap();
        let cola: i64 =
            tx.query_row("SELECT id FROM items WHERE name = 'Cola 500ml'", [], |row| row.get(0)).unwrap();
        create_sale(
            &tx,
            CreateSaleInput {
                items: vec![CartLine { item_id: cola, qty: 3.0, notes: None }],
                discount_minor: 0,
                tax_minor: 0,
                payment_method: "cash".into(),
                cashier_id: None,
                table_id: Some(table_id),
                shift_id: None,
            },
        )
        .unwrap();
        tx.commit().unwrap();

        let (start, end) = wide_range();
        let summary = get_summary(&conn, &start, &end, Platform::Desktop).unwrap();
        let top = summary.top_table_by_sales.expect("a table was sold to");
        assert_eq!(top.table_id, table_id);
        assert_eq!(top.name, table_name);
        assert_eq!(top.total_minor, 3 * 8000);
    }
}
