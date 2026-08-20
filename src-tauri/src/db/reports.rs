//! Daily/range sales reporting: totals, top items, and a day-by-day series
//! for charting. Every query here is read-only and scoped to `sales` /
//! `sale_items` — nothing in this module writes to the database.
//!
//! Date ranges arrive as plain `YYYY-MM-DD` strings and are expanded to full
//! `created_at`-comparable timestamps (`start 00:00:00` .. `end 23:59:59`)
//! before hitting SQL, so every query filters with a direct range comparison
//! on the indexed `created_at` column rather than wrapping it in `date(...)`,
//! which would force a full scan.

use std::collections::HashMap;

use chrono::NaiveDate;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// Reports spanning more than this many days are rejected — a guard against
/// an accidental all-time query on a shop with years of history, not a real
/// product constraint. Raise it if a client legitimately needs longer ranges.
const MAX_RANGE_DAYS: i64 = 400;

#[derive(Debug)]
pub enum ReportError {
    InvalidDateRange(String),
    RangeTooLarge,
    Sqlite(rusqlite::Error),
}

impl std::fmt::Display for ReportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReportError::InvalidDateRange(msg) => write!(f, "{}", msg),
            ReportError::RangeTooLarge => {
                write!(f, "Date range is too large — please narrow it to {} days or fewer", MAX_RANGE_DAYS)
            }
            ReportError::Sqlite(err) => write!(f, "database error: {}", err),
        }
    }
}

impl From<rusqlite::Error> for ReportError {
    fn from(err: rusqlite::Error) -> Self {
        ReportError::Sqlite(err)
    }
}

/// Parses and checks a `start_date`/`end_date` pair, returning the full
/// `created_at`-comparable timestamp bounds for the SQL WHERE clause.
fn validate_range(start_date: &str, end_date: &str) -> Result<(String, String), ReportError> {
    let start = NaiveDate::parse_from_str(start_date, "%Y-%m-%d")
        .map_err(|_| ReportError::InvalidDateRange(format!("Invalid start date: {}", start_date)))?;
    let end = NaiveDate::parse_from_str(end_date, "%Y-%m-%d")
        .map_err(|_| ReportError::InvalidDateRange(format!("Invalid end date: {}", end_date)))?;

    if start > end {
        return Err(ReportError::InvalidDateRange(
            "Start date must not be after the end date".into(),
        ));
    }
    if (end - start).num_days() > MAX_RANGE_DAYS {
        return Err(ReportError::RangeTooLarge);
    }

    Ok((format!("{} 00:00:00", start_date), format!("{} 23:59:59", end_date)))
}

// ---------------------------------------------------------------------------
// Summary
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SalesSummary {
    pub start_date: String,
    pub end_date: String,
    /// Gross — the sum of `sales.total_minor` in the range, unaffected by
    /// any refund. Kept as its own field (rather than folding refunds in
    /// here) so "how much did we ring up" and "how much did we actually
    /// keep" both stay answerable from one summary.
    pub total_sales_minor: i64,
    /// Refunds *recorded* in the range (by `refunds.created_at`, not by
    /// when the original sale happened) — a refund issued today against
    /// last week's sale reduces today's figure, not last week's.
    pub refunds_minor: i64,
    /// `total_sales_minor - refunds_minor` — the number profit/dashboard
    /// math should use, never `total_sales_minor` alone.
    pub net_sales_minor: i64,
    pub transaction_count: i64,
    /// Rounded to the nearest minor unit — never a fraction of a cent.
    /// Computed against gross per-sale totals (a refund isn't "one more
    /// transaction" with its own average), same as `transaction_count`.
    pub average_sale_minor: i64,
}

fn refunds_total(conn: &Connection, start_ts: &str, end_ts: &str) -> Result<i64, rusqlite::Error> {
    conn.query_row(
        "SELECT COALESCE(SUM(total_refund_amount_minor), 0)
           FROM refunds
          WHERE created_at >= ?1 AND created_at <= ?2",
        params![start_ts, end_ts],
        |row| row.get(0),
    )
}

pub fn get_sales_summary(
    conn: &Connection,
    start_date: &str,
    end_date: &str,
) -> Result<SalesSummary, ReportError> {
    let (start_ts, end_ts) = validate_range(start_date, end_date)?;

    let (total_sales_minor, transaction_count): (i64, i64) = conn.query_row(
        "SELECT COALESCE(SUM(total_minor), 0), COUNT(*)
           FROM sales
          WHERE created_at >= ?1 AND created_at <= ?2",
        params![start_ts, end_ts],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let refunds_minor = refunds_total(conn, &start_ts, &end_ts)?;

    let average_sale_minor = if transaction_count > 0 {
        (total_sales_minor as f64 / transaction_count as f64).round() as i64
    } else {
        0
    };

    Ok(SalesSummary {
        start_date: start_date.to_string(),
        end_date: end_date.to_string(),
        total_sales_minor,
        refunds_minor,
        net_sales_minor: total_sales_minor - refunds_minor,
        transaction_count,
        average_sale_minor,
    })
}

// ---------------------------------------------------------------------------
// Top items
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TopItemSort {
    Quantity,
    Revenue,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopItem {
    pub item_id: i64,
    pub item_name: String,
    pub qty_sold: i64,
    pub revenue_minor: i64,
}

pub fn get_top_items(
    conn: &Connection,
    start_date: &str,
    end_date: &str,
    limit: i64,
    sort_by: TopItemSort,
) -> Result<Vec<TopItem>, ReportError> {
    let (start_ts, end_ts) = validate_range(start_date, end_date)?;
    let limit = limit.clamp(1, 100);

    // `sort_by` is a closed Rust enum, never user-supplied text, so it is
    // safe to interpolate its matched column name directly into the SQL text.
    let order_by = match sort_by {
        TopItemSort::Quantity => "qty_sold DESC, revenue_minor DESC",
        TopItemSort::Revenue => "revenue_minor DESC, qty_sold DESC",
    };
    let sql = format!(
        "SELECT si.item_id, i.name, SUM(si.qty) AS qty_sold,
                SUM(si.qty * si.price_at_sale_minor) AS revenue_minor
           FROM sale_items si
           JOIN sales s ON s.id = si.sale_id
           JOIN items i ON i.id = si.item_id
          WHERE s.created_at >= ?1 AND s.created_at <= ?2
          GROUP BY si.item_id, i.name
          ORDER BY {}
          LIMIT ?3",
        order_by
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![start_ts, end_ts, limit], |row| {
        Ok(TopItem {
            item_id: row.get(0)?,
            item_name: row.get(1)?,
            qty_sold: row.get(2)?,
            revenue_minor: row.get(3)?,
        })
    })?;

    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

// ---------------------------------------------------------------------------
// Category-wise sale — one grouped section per category (header, item rows,
// subtotal), grand total at the end. Feeds both the PDF export and the
// print templates (ESC/POS + PDF) built on top of it.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategorySalesLine {
    pub item_id: i64,
    pub item_name: String,
    pub qty_sold: i64,
    pub revenue_minor: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategorySalesGroup {
    /// `None` for items with no category — grouped under `category_name:
    /// "Uncategorized"` rather than dropped from the report.
    pub category_id: Option<i64>,
    pub category_name: String,
    pub items: Vec<CategorySalesLine>,
    pub subtotal_minor: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategorySalesReport {
    pub start_date: String,
    pub end_date: String,
    pub groups: Vec<CategorySalesGroup>,
    pub grand_total_minor: i64,
}

/// Every item sold in the range, grouped by category (highest-revenue
/// category first), each with a per-item breakdown and a subtotal — the
/// "Category Wise Sale" report.
pub fn get_category_sales(
    conn: &Connection,
    start_date: &str,
    end_date: &str,
) -> Result<CategorySalesReport, ReportError> {
    let (start_ts, end_ts) = validate_range(start_date, end_date)?;

    let mut stmt = conn.prepare(
        "SELECT i.category_id, COALESCE(c.name, 'Uncategorized') AS category_name,
                si.item_id, i.name, SUM(si.qty) AS qty_sold,
                SUM(si.qty * si.price_at_sale_minor) AS revenue_minor
           FROM sale_items si
           JOIN sales s ON s.id = si.sale_id
           JOIN items i ON i.id = si.item_id
           LEFT JOIN categories c ON c.id = i.category_id
          WHERE s.created_at >= ?1 AND s.created_at <= ?2
          GROUP BY i.category_id, si.item_id
          ORDER BY category_name, i.name",
    )?;
    let rows: Vec<(Option<i64>, String, i64, String, i64, i64)> = stmt
        .query_map(params![start_ts, end_ts], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut groups: Vec<CategorySalesGroup> = Vec::new();
    for (category_id, category_name, item_id, item_name, qty_sold, revenue_minor) in rows {
        let line = CategorySalesLine { item_id, item_name, qty_sold, revenue_minor };
        match groups.iter_mut().find(|g| g.category_id == category_id) {
            Some(group) => {
                group.subtotal_minor += revenue_minor;
                group.items.push(line);
            }
            None => {
                groups.push(CategorySalesGroup {
                    category_id,
                    category_name,
                    subtotal_minor: revenue_minor,
                    items: vec![line],
                });
            }
        }
    }
    // Highest-earning category first — the report reads top-down as "what
    // actually drove revenue", not an arbitrary/alphabetical order.
    groups.sort_by(|a, b| b.subtotal_minor.cmp(&a.subtotal_minor));

    let grand_total_minor = groups.iter().map(|g| g.subtotal_minor).sum();

    Ok(CategorySalesReport {
        start_date: start_date.to_string(),
        end_date: end_date.to_string(),
        groups,
        grand_total_minor,
    })
}

// ---------------------------------------------------------------------------
// Sales over time
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailySales {
    /// `YYYY-MM-DD`.
    pub date: String,
    /// Gross for the day — see `SalesSummary::total_sales_minor`.
    pub total_minor: i64,
    pub refunds_minor: i64,
    /// `total_minor - refunds_minor`.
    pub net_minor: i64,
    pub transaction_count: i64,
}

/// One row per calendar day in the range, in order, with zero-filled gaps —
/// a day with no sales still appears so the chart never silently skips it.
pub fn get_sales_over_time(
    conn: &Connection,
    start_date: &str,
    end_date: &str,
) -> Result<Vec<DailySales>, ReportError> {
    let (start_ts, end_ts) = validate_range(start_date, end_date)?;
    // Already validated by validate_range above.
    let start = NaiveDate::parse_from_str(start_date, "%Y-%m-%d").unwrap();
    let end = NaiveDate::parse_from_str(end_date, "%Y-%m-%d").unwrap();

    let mut stmt = conn.prepare(
        "SELECT substr(created_at, 1, 10) AS day, COALESCE(SUM(total_minor), 0), COUNT(*)
           FROM sales
          WHERE created_at >= ?1 AND created_at <= ?2
          GROUP BY day",
    )?;
    let by_day: HashMap<String, (i64, i64)> = stmt
        .query_map(params![start_ts, end_ts], |row| {
            Ok((row.get::<_, String>(0)?, (row.get(1)?, row.get(2)?)))
        })?
        .collect::<Result<_, _>>()?;

    let mut refund_stmt = conn.prepare(
        "SELECT substr(created_at, 1, 10) AS day, COALESCE(SUM(total_refund_amount_minor), 0)
           FROM refunds
          WHERE created_at >= ?1 AND created_at <= ?2
          GROUP BY day",
    )?;
    let refunds_by_day: HashMap<String, i64> = refund_stmt
        .query_map(params![start_ts, end_ts], |row| Ok((row.get::<_, String>(0)?, row.get(1)?)))?
        .collect::<Result<_, _>>()?;

    let mut result = Vec::new();
    let mut day = start;
    while day <= end {
        let key = day.format("%Y-%m-%d").to_string();
        let (total_minor, transaction_count) = by_day.get(&key).copied().unwrap_or((0, 0));
        let refunds_minor = refunds_by_day.get(&key).copied().unwrap_or(0);
        result.push(DailySales {
            date: key,
            total_minor,
            refunds_minor,
            net_minor: total_minor - refunds_minor,
            transaction_count,
        });
        day = day.succ_opt().expect("date overflow within a bounded range");
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::test_conn;
    use chrono::{Duration, Local};

    /// Mirrors `seed::date_days_ago` so tests can build ranges around the
    /// same relative dates the seed data was generated from.
    fn days_ago(n: i64) -> String {
        (Local::now() - Duration::days(n)).format("%Y-%m-%d").to_string()
    }

    #[test]
    fn summary_over_the_full_seeded_range_matches_all_twelve_sales() {
        let conn = test_conn();
        let summary = get_sales_summary(&conn, &days_ago(30), &days_ago(0)).unwrap();
        assert_eq!(summary.transaction_count, 12);

        let (total, count): (i64, i64) = conn
            .query_row("SELECT SUM(total_minor), COUNT(*) FROM sales", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(summary.total_sales_minor, total);
        assert_eq!(summary.average_sale_minor, (total as f64 / count as f64).round() as i64);
    }

    #[test]
    fn summary_narrows_correctly_to_a_single_day() {
        let conn = test_conn();
        // Seed data has exactly one sale "today" (days_ago(0)).
        let summary = get_sales_summary(&conn, &days_ago(0), &days_ago(0)).unwrap();
        assert_eq!(summary.transaction_count, 1);

        let expected_total: i64 = conn
            .query_row(
                "SELECT total_minor FROM sales WHERE substr(created_at,1,10) = date('now','localtime')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(summary.total_sales_minor, expected_total);
    }

    #[test]
    fn summary_is_zero_for_a_range_with_no_sales() {
        let conn = test_conn();
        let far_future = (Local::now() + Duration::days(365)).format("%Y-%m-%d").to_string();
        let summary = get_sales_summary(&conn, &far_future, &far_future).unwrap();
        assert_eq!(summary.transaction_count, 0);
        assert_eq!(summary.total_sales_minor, 0);
        assert_eq!(summary.average_sale_minor, 0, "must not divide by zero");
    }

    #[test]
    fn rejects_a_start_date_after_the_end_date() {
        let conn = test_conn();
        let err = get_sales_summary(&conn, &days_ago(0), &days_ago(5)).unwrap_err();
        assert!(matches!(err, ReportError::InvalidDateRange(_)));
    }

    #[test]
    fn rejects_a_malformed_date() {
        let conn = test_conn();
        let err = get_sales_summary(&conn, "not-a-date", &days_ago(0)).unwrap_err();
        assert!(matches!(err, ReportError::InvalidDateRange(_)));
    }

    #[test]
    fn rejects_a_range_that_is_too_large() {
        let conn = test_conn();
        let start = (Local::now() - Duration::days(500)).format("%Y-%m-%d").to_string();
        let err = get_sales_summary(&conn, &start, &days_ago(0)).unwrap_err();
        assert!(matches!(err, ReportError::RangeTooLarge));
    }

    #[test]
    fn top_items_by_quantity_orders_the_best_seller_first() {
        let conn = test_conn();
        let items = get_top_items(&conn, &days_ago(30), &days_ago(0), 5, TopItemSort::Quantity).unwrap();
        assert!(!items.is_empty());
        for pair in items.windows(2) {
            assert!(pair[0].qty_sold >= pair[1].qty_sold, "must be sorted by quantity descending");
        }
    }

    #[test]
    fn top_items_by_revenue_can_differ_in_order_from_by_quantity() {
        let conn = test_conn();
        let by_qty = get_top_items(&conn, &days_ago(30), &days_ago(0), 20, TopItemSort::Quantity).unwrap();
        let by_rev = get_top_items(&conn, &days_ago(30), &days_ago(0), 20, TopItemSort::Revenue).unwrap();

        for pair in by_rev.windows(2) {
            assert!(pair[0].revenue_minor >= pair[1].revenue_minor);
        }
        // Same underlying set of items, just not necessarily the same order —
        // a cheap high-volume item and an expensive low-volume one should be
        // able to swap places between the two rankings.
        let qty_ids: std::collections::HashSet<_> = by_qty.iter().map(|i| i.item_id).collect();
        let rev_ids: std::collections::HashSet<_> = by_rev.iter().map(|i| i.item_id).collect();
        assert_eq!(qty_ids, rev_ids);
    }

    #[test]
    fn top_items_respects_the_limit() {
        let conn = test_conn();
        let items = get_top_items(&conn, &days_ago(30), &days_ago(0), 2, TopItemSort::Quantity).unwrap();
        assert!(items.len() <= 2);
    }

    #[test]
    fn top_items_revenue_matches_qty_times_price_at_sale() {
        let conn = test_conn();
        let items = get_top_items(&conn, &days_ago(30), &days_ago(0), 50, TopItemSort::Quantity).unwrap();
        for item in &items {
            let (qty, revenue): (i64, i64) = conn
                .query_row(
                    "SELECT SUM(qty), SUM(qty * price_at_sale_minor) FROM sale_items WHERE item_id = ?1",
                    params![item.item_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(item.qty_sold, qty);
            assert_eq!(item.revenue_minor, revenue);
        }
    }

    #[test]
    fn sales_over_time_has_one_row_per_day_with_no_gaps() {
        let conn = test_conn();
        let series = get_sales_over_time(&conn, &days_ago(5), &days_ago(0)).unwrap();
        assert_eq!(series.len(), 6, "5 days ago through today, inclusive, is 6 days");
        for pair in series.windows(2) {
            assert!(pair[0].date < pair[1].date, "days must be in ascending order");
        }
    }

    #[test]
    fn sales_over_time_zero_fills_a_day_with_no_sales() {
        let conn = test_conn();
        // Seed data has no sale at exactly 4 or 11 days ago (gaps between the
        // seeded clusters of 12/9/6/3/1/0) — assert the gap is present, not skipped.
        let series = get_sales_over_time(&conn, &days_ago(11), &days_ago(11)).unwrap();
        assert_eq!(series.len(), 1);
        assert_eq!(series[0].transaction_count, 0);
        assert_eq!(series[0].total_minor, 0);
    }

    #[test]
    fn sales_over_time_totals_match_the_summary_for_the_same_range() {
        let conn = test_conn();
        let summary = get_sales_summary(&conn, &days_ago(30), &days_ago(0)).unwrap();
        let series = get_sales_over_time(&conn, &days_ago(30), &days_ago(0)).unwrap();

        let series_total: i64 = series.iter().map(|d| d.total_minor).sum();
        let series_count: i64 = series.iter().map(|d| d.transaction_count).sum();
        assert_eq!(series_total, summary.total_sales_minor);
        assert_eq!(series_count, summary.transaction_count);
    }

    /// Rings up a fresh Cola sale today, for tests that need a refund to
    /// hang off something real without depending on which seeded sale IDs
    /// exist.
    fn seed_sale_today(conn: &mut Connection) -> (i64, i64) {
        use crate::db::sales::{create_sale, CartLine, CreateSaleInput};
        let tx = conn.transaction().unwrap();
        let cola: i64 =
            tx.query_row("SELECT id FROM items WHERE name = 'Cola 500ml'", [], |row| row.get(0)).unwrap();
        let sale = create_sale(
            &tx,
            CreateSaleInput {
                items: vec![CartLine { item_id: cola, qty: 2, notes: None }],
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
    }

    #[test]
    fn a_refund_today_reduces_net_sales_but_not_gross() {
        use crate::db::refunds::{create_refund, CreateRefundInput, RefundLineInput};
        let mut conn = test_conn();
        let (sale_id, sale_item_id) = seed_sale_today(&mut conn);

        let before = get_sales_summary(&conn, &days_ago(0), &days_ago(0)).unwrap();

        {
            let tx = conn.transaction().unwrap();
            create_refund(
                &tx,
                CreateRefundInput {
                    sale_id,
                    items: vec![RefundLineInput { sale_item_id, qty: 1, amount_minor: 8000 }],
                    reason: None,
                    refunded_by: None,
                },
            )
            .unwrap();
            tx.commit().unwrap();
        }

        let after = get_sales_summary(&conn, &days_ago(0), &days_ago(0)).unwrap();
        assert_eq!(after.total_sales_minor, before.total_sales_minor, "gross is untouched by a refund");
        assert_eq!(after.refunds_minor, before.refunds_minor + 8000);
        assert_eq!(after.net_sales_minor, after.total_sales_minor - 8000);
    }

    #[test]
    fn a_refund_appears_on_the_day_it_was_recorded_not_the_original_sale_day() {
        use crate::db::refunds::{create_refund, CreateRefundInput, RefundLineInput};
        let mut conn = test_conn();
        // Seed data has a sale 12 days ago; refund it "today" instead.
        let sale_id: i64 = conn
            .query_row(
                "SELECT id FROM sales WHERE substr(created_at,1,10) = ?1 LIMIT 1",
                params![days_ago(12)],
                |row| row.get(0),
            )
            .unwrap();
        let sale_item_id: i64 = conn
            .query_row("SELECT id FROM sale_items WHERE sale_id = ?1 LIMIT 1", params![sale_id], |row| row.get(0))
            .unwrap();
        let price: i64 = conn
            .query_row("SELECT price_at_sale_minor FROM sale_items WHERE id = ?1", params![sale_item_id], |row| {
                row.get(0)
            })
            .unwrap();

        {
            let tx = conn.transaction().unwrap();
            create_refund(
                &tx,
                CreateRefundInput {
                    sale_id,
                    items: vec![RefundLineInput { sale_item_id, qty: 1, amount_minor: price }],
                    reason: None,
                    refunded_by: None,
                },
            )
            .unwrap();
            tx.commit().unwrap();
        }

        let today = get_sales_summary(&conn, &days_ago(0), &days_ago(0)).unwrap();
        let twelve_days_ago = get_sales_summary(&conn, &days_ago(12), &days_ago(12)).unwrap();
        assert_eq!(today.refunds_minor, price, "the refund lands on today's report");
        assert_eq!(twelve_days_ago.refunds_minor, 0, "not on the original sale's day");
    }

    #[test]
    fn category_sales_groups_by_category_with_correct_subtotals_and_grand_total() {
        let conn = test_conn();
        let report = get_category_sales(&conn, &days_ago(30), &days_ago(0)).unwrap();

        assert!(!report.groups.is_empty(), "seed data has sales across categories");
        for group in &report.groups {
            let sum: i64 = group.items.iter().map(|i| i.revenue_minor).sum();
            assert_eq!(sum, group.subtotal_minor, "{}'s subtotal must equal its own item sum", group.category_name);
        }
        let expected_grand_total: i64 = report.groups.iter().map(|g| g.subtotal_minor).sum();
        assert_eq!(report.grand_total_minor, expected_grand_total);

        // Same underlying revenue as get_top_items over the same range.
        let top_items_total: i64 = get_top_items(&conn, &days_ago(30), &days_ago(0), 1000, TopItemSort::Revenue)
            .unwrap()
            .iter()
            .map(|i| i.revenue_minor)
            .sum();
        assert_eq!(report.grand_total_minor, top_items_total);
    }

    #[test]
    fn category_sales_groups_are_sorted_by_revenue_descending() {
        let conn = test_conn();
        let report = get_category_sales(&conn, &days_ago(30), &days_ago(0)).unwrap();
        for pair in report.groups.windows(2) {
            assert!(pair[0].subtotal_minor >= pair[1].subtotal_minor);
        }
    }

    #[test]
    fn category_sales_is_empty_for_a_range_with_no_sales() {
        let conn = test_conn();
        let far_future = (chrono::Local::now() + Duration::days(365)).format("%Y-%m-%d").to_string();
        let report = get_category_sales(&conn, &far_future, &far_future).unwrap();
        assert!(report.groups.is_empty());
        assert_eq!(report.grand_total_minor, 0);
    }
}
