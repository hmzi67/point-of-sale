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
    pub total_sales_minor: i64,
    pub transaction_count: i64,
    /// Rounded to the nearest minor unit — never a fraction of a cent.
    pub average_sale_minor: i64,
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

    let average_sale_minor = if transaction_count > 0 {
        (total_sales_minor as f64 / transaction_count as f64).round() as i64
    } else {
        0
    };

    Ok(SalesSummary {
        start_date: start_date.to_string(),
        end_date: end_date.to_string(),
        total_sales_minor,
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
// Sales over time
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailySales {
    /// `YYYY-MM-DD`.
    pub date: String,
    pub total_minor: i64,
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

    let mut result = Vec::new();
    let mut day = start;
    while day <= end {
        let key = day.format("%Y-%m-%d").to_string();
        let (total_minor, transaction_count) = by_day.get(&key).copied().unwrap_or((0, 0));
        result.push(DailySales { date: key, total_minor, transaction_count });
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
}
