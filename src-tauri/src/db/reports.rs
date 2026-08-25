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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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
// Product Wise Sales — a full per-item breakdown across the whole catalog,
// not just a top-N (contrast with `get_top_items`): every item that sold at
// least once in the range, ranked by revenue or quantity, plus a separate
// "no sales this period" section listing active items that sold zero units
// so slow-moving stock stays visible instead of just silently absent.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductSalesRow {
    pub item_id: i64,
    pub item_name: String,
    pub category_id: Option<i64>,
    pub category_name: String,
    pub qty_sold: i64,
    pub revenue_minor: i64,
    /// 1-based position under the requested `sort_by`, computed here so the
    /// frontend never has to re-derive it (and can't get it wrong once the
    /// table is paginated client-side).
    pub rank: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductSalesNoSaleRow {
    pub item_id: i64,
    pub item_name: String,
    pub category_id: Option<i64>,
    pub category_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductSalesSummaryReport {
    pub start_date: String,
    pub end_date: String,
    pub sort_by: TopItemSort,
    /// Every item with at least one sale in the range, ranked 1..N.
    pub rows: Vec<ProductSalesRow>,
    /// Active items with zero sales in the range, alphabetical — a plain
    /// list rather than ranked, since there is nothing to rank them by.
    pub no_sales_items: Vec<ProductSalesNoSaleRow>,
}

/// The "Product Wise Sales" report: every item sold in `start_date`..
/// `end_date`, optionally narrowed to one category, ranked by `sort_by` —
/// plus every active item that sold zero units in the same range/category,
/// so a slow mover shows up as a visible zero rather than being hidden by
/// only ever reporting "what sold".
///
/// Same join shape as `get_top_items` (`sale_items` -> `sales` -> `items`),
/// so it rides the same `idx_sale_items_item` / `idx_sales_created_at`
/// indexes already proven index-backed by `hot_path_queries_are_index_backed`
/// — no new index needed for this query.
pub fn get_product_sales_summary(
    conn: &Connection,
    start_date: &str,
    end_date: &str,
    category_id: Option<i64>,
    sort_by: TopItemSort,
) -> Result<ProductSalesSummaryReport, ReportError> {
    let (start_ts, end_ts) = validate_range(start_date, end_date)?;

    // `sort_by` is a closed Rust enum, never user-supplied text — safe to
    // interpolate its matched column list directly into the SQL text.
    let order_by = match sort_by {
        TopItemSort::Quantity => "qty_sold DESC, revenue_minor DESC, i.name",
        TopItemSort::Revenue => "revenue_minor DESC, qty_sold DESC, i.name",
    };
    let sql = format!(
        "SELECT si.item_id, i.name, i.category_id, COALESCE(c.name, 'Uncategorized') AS category_name,
                SUM(si.qty) AS qty_sold, SUM(si.qty * si.price_at_sale_minor) AS revenue_minor
           FROM sale_items si
           JOIN sales s ON s.id = si.sale_id
           JOIN items i ON i.id = si.item_id
           LEFT JOIN categories c ON c.id = i.category_id
          WHERE s.created_at >= ?1 AND s.created_at <= ?2
            AND (?3 IS NULL OR i.category_id = ?3)
          GROUP BY si.item_id
          ORDER BY {}",
        order_by
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut rows: Vec<ProductSalesRow> = stmt
        .query_map(params![start_ts, end_ts, category_id], |row| {
            Ok(ProductSalesRow {
                item_id: row.get(0)?,
                item_name: row.get(1)?,
                category_id: row.get(2)?,
                category_name: row.get(3)?,
                qty_sold: row.get(4)?,
                revenue_minor: row.get(5)?,
                rank: 0,
            })
        })?
        .collect::<Result<_, _>>()?;
    for (i, row) in rows.iter_mut().enumerate() {
        row.rank = (i + 1) as i64;
    }

    let mut no_sales_stmt = conn.prepare(
        "SELECT i.id, i.name, i.category_id, COALESCE(c.name, 'Uncategorized')
           FROM items i
           LEFT JOIN categories c ON c.id = i.category_id
          WHERE i.is_active = 1
            AND (?1 IS NULL OR i.category_id = ?1)
            AND i.id NOT IN (
                SELECT si.item_id FROM sale_items si
                  JOIN sales s ON s.id = si.sale_id
                 WHERE s.created_at >= ?2 AND s.created_at <= ?3
            )
          ORDER BY i.name",
    )?;
    let no_sales_items: Vec<ProductSalesNoSaleRow> = no_sales_stmt
        .query_map(params![category_id, start_ts, end_ts], |row| {
            Ok(ProductSalesNoSaleRow {
                item_id: row.get(0)?,
                item_name: row.get(1)?,
                category_id: row.get(2)?,
                category_name: row.get(3)?,
            })
        })?
        .collect::<Result<_, _>>()?;

    Ok(ProductSalesSummaryReport {
        start_date: start_date.to_string(),
        end_date: end_date.to_string(),
        sort_by,
        rows,
        no_sales_items,
    })
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
// Table-wise sale — one row per table plus a "Counter / Takeaway" row for
// sales with no table (`sales.table_id IS NULL`), so the rows always sum to
// the same total `get_sales_summary` reports for the same range — nothing
// falls between the two. Only meaningful when the `tables` module is in
// use; like every other report in this file, that's a frontend/route
// concern (see `ModuleRoute`), not something this query itself checks.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableSalesRow {
    /// `None` for the "Counter / Takeaway" row.
    pub table_id: Option<i64>,
    pub label: String,
    pub total_minor: i64,
    pub transaction_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableSalesSummary {
    pub start_date: String,
    pub end_date: String,
    /// Highest-total first, "Counter / Takeaway" sorted in among the tables
    /// like any other row rather than pinned to the top or bottom.
    pub rows: Vec<TableSalesRow>,
    pub grand_total_minor: i64,
}

/// The label for sales with no table — deliberately not just "Counter" or
/// just "Takeaway", since a retail client's non-table sales cover both and
/// there's no `order_type` column to distinguish them: the billing UI's
/// "Takeaway" is derived purely from `table_id` being unset, never stored
/// as its own value (see `OrderTypeAndTable.tsx`'s doc comment).
const NO_TABLE_LABEL: &str = "Counter / Takeaway";

pub fn get_table_sales_summary(
    conn: &Connection,
    start_date: &str,
    end_date: &str,
) -> Result<TableSalesSummary, ReportError> {
    let (start_ts, end_ts) = validate_range(start_date, end_date)?;

    let mut stmt = conn.prepare(
        "SELECT s.table_id, t.name, SUM(s.total_minor), COUNT(*)
           FROM sales s
           LEFT JOIN tables t ON t.id = s.table_id
          WHERE s.created_at >= ?1 AND s.created_at <= ?2
          GROUP BY s.table_id",
    )?;
    let mut rows: Vec<TableSalesRow> = stmt
        .query_map(params![start_ts, end_ts], |row| {
            let table_id: Option<i64> = row.get(0)?;
            let name: Option<String> = row.get(1)?;
            Ok(TableSalesRow {
                table_id,
                label: name.unwrap_or_else(|| NO_TABLE_LABEL.to_string()),
                total_minor: row.get(2)?,
                transaction_count: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    // Highest sales first — same "what actually drove revenue" ordering
    // get_category_sales uses.
    rows.sort_by(|a, b| b.total_minor.cmp(&a.total_minor));

    let grand_total_minor = rows.iter().map(|r| r.total_minor).sum();

    Ok(TableSalesSummary { start_date: start_date.to_string(), end_date: end_date.to_string(), rows, grand_total_minor })
}

// ---------------------------------------------------------------------------
// Refunds
// ---------------------------------------------------------------------------

/// One refund for the report — reuses `refunds::Refund`/`RefundLine` (the
/// same shape the single-refund receipt/reprint already return) rather than
/// a parallel type, so the frontend's existing `Refund` type covers this
/// report's rows for free and the two can never drift in shape.
use crate::db::refunds::{Refund, RefundLine};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefundsSummary {
    pub start_date: String,
    pub end_date: String,
    /// Most recent first — matches the on-screen table's default sort.
    pub refunds: Vec<Refund>,
    pub grand_total_refunded_minor: i64,
}

/// Every refund recorded in the range (by refund date, not the date of the
/// original sale — same convention `DashboardSummary.refunds_minor` and
/// `SalesSummary.refunds_minor` already use), most recent first, plus a
/// grand total. Not module-gated — refunds aren't a toggleable module, same
/// as the Overview's refunds figure.
pub fn get_refunds_summary(
    conn: &Connection,
    start_date: &str,
    end_date: &str,
) -> Result<RefundsSummary, ReportError> {
    let (start_ts, end_ts) = validate_range(start_date, end_date)?;

    let mut header_stmt = conn.prepare(
        "SELECT r.id, r.original_sale_id, r.refunded_by, u.name, r.reason,
                r.total_refund_amount_minor, r.created_at
           FROM refunds r
           LEFT JOIN users u ON u.id = r.refunded_by
          WHERE r.created_at >= ?1 AND r.created_at <= ?2
          ORDER BY r.created_at DESC, r.id DESC",
    )?;
    #[allow(clippy::type_complexity)]
    let headers: Vec<(i64, i64, Option<i64>, Option<String>, Option<String>, i64, String)> = header_stmt
        .query_map(params![start_ts, end_ts], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut item_stmt = conn.prepare(
        "SELECT ri.sale_item_id, si.item_id, i.name, ri.qty_refunded, ri.amount_refunded_minor
           FROM refund_items ri
           JOIN sale_items si ON si.id = ri.sale_item_id
           JOIN items i ON i.id = si.item_id
          WHERE ri.refund_id = ?1
          ORDER BY ri.id",
    )?;

    let mut refunds = Vec::with_capacity(headers.len());
    let mut grand_total_refunded_minor: i64 = 0;
    for (id, original_sale_id, refunded_by, refunded_by_name, reason, total_refund_amount_minor, created_at) in headers
    {
        let items = item_stmt
            .query_map(params![id], |row| {
                Ok(RefundLine {
                    sale_item_id: row.get(0)?,
                    item_id: row.get(1)?,
                    item_name: row.get(2)?,
                    qty_refunded: row.get(3)?,
                    amount_refunded_minor: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        grand_total_refunded_minor += total_refund_amount_minor;
        refunds.push(Refund {
            id,
            original_sale_id,
            refunded_by,
            refunded_by_name,
            reason,
            total_refund_amount_minor,
            created_at,
            items,
        });
    }

    Ok(RefundsSummary {
        start_date: start_date.to_string(),
        end_date: end_date.to_string(),
        refunds,
        grand_total_refunded_minor,
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

// ---------------------------------------------------------------------------
// Best sellers — a live, recomputed-on-every-load ranking (never a stored
// flag) feeding the "fire" badge on Inventory/Billing item cards. Offline
// software has no background job scheduler, so this is a plain query run on
// screen load rather than anything cron-like.
// ---------------------------------------------------------------------------

/// Minimum total quantity sold within the window before an item counts as a
/// "best seller" at all. Without a floor, a brand-new client with only a
/// handful of sales would get an arbitrary badge on whatever 1-2 items
/// happened to sell once — this keeps the badge meaningless until an item
/// has genuinely moved some real volume.
const BEST_SELLER_MIN_QTY: i64 = 5;

/// Top-selling item ids by quantity sold in the last `period_days` (a
/// rolling window ending now, not a calendar range), highest quantity first,
/// capped at `limit` — the "best seller" set for the fire badge. Items below
/// `BEST_SELLER_MIN_QTY` total units in the window never qualify, so a shop
/// with little/no sales history gets no badges at all rather than random ones.
pub fn get_best_selling_item_ids(
    conn: &Connection,
    period_days: i64,
    limit: i64,
) -> Result<Vec<i64>, ReportError> {
    let period_days = period_days.clamp(1, MAX_RANGE_DAYS);
    let limit = limit.clamp(1, 50);
    let window_start = (chrono::Local::now() - chrono::Duration::days(period_days))
        .format("%Y-%m-%d 00:00:00")
        .to_string();

    let mut stmt = conn.prepare(
        "SELECT si.item_id, SUM(si.qty) AS qty_sold
           FROM sale_items si
           JOIN sales s ON s.id = si.sale_id
          WHERE s.created_at >= ?1
          GROUP BY si.item_id
         HAVING qty_sold >= ?2
          ORDER BY qty_sold DESC
          LIMIT ?3",
    )?;
    let ids = stmt
        .query_map(params![window_start, BEST_SELLER_MIN_QTY, limit], |row| row.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ids)
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

    /// Rings up one dine-in sale against a real table, alongside the seed
    /// data's counter sales — a clean way to exercise a genuine table row,
    /// since none of the seeded sales are table-linked.
    fn seed_one_table_sale(conn: &mut Connection) {
        use crate::db::sales::{create_sale, CartLine, CreateSaleInput};
        use crate::db::tables::start_table_order;

        let tx = conn.transaction().unwrap();
        let table_id: i64 =
            tx.query_row("SELECT id FROM tables LIMIT 1", [], |row| row.get(0)).unwrap();
        start_table_order(&tx, table_id).unwrap();
        let cola: i64 =
            tx.query_row("SELECT id FROM items WHERE name = 'Cola 500ml'", [], |row| row.get(0)).unwrap();
        create_sale(
            &tx,
            CreateSaleInput {
                items: vec![CartLine { item_id: cola, qty: 1, notes: None }],
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
    }

    /// The exact reconciliation the product spec calls for: every table row
    /// plus the "Counter / Takeaway" row must sum to precisely what
    /// `get_sales_summary` reports as the gross total for the same range —
    /// nothing double-counted, nothing dropped.
    #[test]
    fn table_sales_rows_reconcile_with_the_overall_summary_for_the_same_range() {
        let mut conn = test_conn();
        seed_one_table_sale(&mut conn);

        let range = (days_ago(30), days_ago(0));
        let summary = get_sales_summary(&conn, &range.0, &range.1).unwrap();
        let table_sales = get_table_sales_summary(&conn, &range.0, &range.1).unwrap();

        assert_eq!(
            table_sales.grand_total_minor, summary.total_sales_minor,
            "table-wise rows must sum to the same gross total the overall summary reports"
        );
        let rows_sum: i64 = table_sales.rows.iter().map(|r| r.total_minor).sum();
        assert_eq!(rows_sum, table_sales.grand_total_minor);

        let counter_row = table_sales.rows.iter().find(|r| r.table_id.is_none());
        assert!(counter_row.is_some(), "seed data's non-table sales must appear as a Counter/Takeaway row");
        let table_row = table_sales.rows.iter().find(|r| r.table_id.is_some());
        assert!(table_row.is_some(), "the one dine-in sale must appear as its own table row");
    }

    #[test]
    fn table_sales_rows_are_sorted_by_amount_descending() {
        let mut conn = test_conn();
        seed_one_table_sale(&mut conn);

        let table_sales = get_table_sales_summary(&conn, &days_ago(30), &days_ago(0)).unwrap();
        for pair in table_sales.rows.windows(2) {
            assert!(pair[0].total_minor >= pair[1].total_minor);
        }
    }

    #[test]
    fn table_sales_uses_the_counter_takeaway_label_for_untabled_sales() {
        let conn = test_conn(); // seed data is all non-table sales
        let table_sales = get_table_sales_summary(&conn, &days_ago(30), &days_ago(0)).unwrap();
        assert_eq!(table_sales.rows.len(), 1, "only the Counter/Takeaway row, no real tables were sold to");
        assert_eq!(table_sales.rows[0].label, "Counter / Takeaway");
        assert_eq!(table_sales.rows[0].table_id, None);
    }

    #[test]
    fn table_sales_is_empty_for_a_range_with_no_sales() {
        let conn = test_conn();
        let far_future = (Local::now() + Duration::days(365)).format("%Y-%m-%d").to_string();
        let table_sales = get_table_sales_summary(&conn, &far_future, &far_future).unwrap();
        assert!(table_sales.rows.is_empty());
        assert_eq!(table_sales.grand_total_minor, 0);
    }

    #[test]
    fn product_sales_summary_ranks_sold_items_and_lists_the_rest_as_no_sales() {
        let conn = test_conn();
        let report =
            get_product_sales_summary(&conn, &days_ago(30), &days_ago(0), None, TopItemSort::Revenue).unwrap();

        assert!(!report.rows.is_empty(), "seed data has sales");
        for (i, row) in report.rows.iter().enumerate() {
            assert_eq!(row.rank, (i + 1) as i64);
        }
        for pair in report.rows.windows(2) {
            assert!(pair[0].revenue_minor >= pair[1].revenue_minor, "must be ranked by revenue descending");
        }

        // Every active item is in exactly one of the two lists, never both.
        let sold_ids: std::collections::HashSet<_> = report.rows.iter().map(|r| r.item_id).collect();
        for no_sale in &report.no_sales_items {
            assert!(!sold_ids.contains(&no_sale.item_id), "an item can't be both sold and no-sale for the same range");
        }

        let total_active: i64 =
            conn.query_row("SELECT COUNT(*) FROM items WHERE is_active = 1", [], |row| row.get(0)).unwrap();
        assert_eq!(report.rows.len() as i64 + report.no_sales_items.len() as i64, total_active);
    }

    #[test]
    fn product_sales_summary_sort_order_changes_with_sort_by() {
        let conn = test_conn();
        let by_qty =
            get_product_sales_summary(&conn, &days_ago(30), &days_ago(0), None, TopItemSort::Quantity).unwrap();
        for pair in by_qty.rows.windows(2) {
            assert!(pair[0].qty_sold >= pair[1].qty_sold);
        }
    }

    #[test]
    fn product_sales_summary_narrows_to_one_category() {
        let conn = test_conn();
        let category_id: i64 = conn
            .query_row("SELECT category_id FROM items WHERE category_id IS NOT NULL LIMIT 1", [], |row| row.get(0))
            .unwrap();
        let report = get_product_sales_summary(
            &conn,
            &days_ago(30),
            &days_ago(0),
            Some(category_id),
            TopItemSort::Revenue,
        )
        .unwrap();
        for row in &report.rows {
            assert_eq!(row.category_id, Some(category_id));
        }
        for row in &report.no_sales_items {
            assert_eq!(row.category_id, Some(category_id));
        }
    }

    #[test]
    fn product_sales_summary_is_all_no_sales_for_a_range_with_no_sales() {
        let conn = test_conn();
        let far_future = (Local::now() + Duration::days(365)).format("%Y-%m-%d").to_string();
        let report =
            get_product_sales_summary(&conn, &far_future, &far_future, None, TopItemSort::Revenue).unwrap();
        assert!(report.rows.is_empty());
        assert!(!report.no_sales_items.is_empty(), "every active item should show up as a zero-sales row");
    }

    #[test]
    fn best_sellers_requires_the_minimum_quantity_floor() {
        let conn = test_conn();
        // A very high floor via a tiny limit still respects HAVING — assert
        // every id returned genuinely cleared the floor by checking none of
        // them are absent from a manual qty sum above the floor.
        let ids = get_best_selling_item_ids(&conn, 30, 5).unwrap();
        for id in &ids {
            let qty: i64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(si.qty), 0) FROM sale_items si
                       JOIN sales s ON s.id = si.sale_id
                      WHERE si.item_id = ?1 AND s.created_at >= datetime('now', '-30 days')",
                    params![id],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(qty >= BEST_SELLER_MIN_QTY, "item {} only sold {} units, below the best-seller floor", id, qty);
        }
    }

    #[test]
    fn best_sellers_is_empty_for_a_brand_new_database_with_no_sales() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        // A fresh install with schema applied but demo-seed data skipped
        // (simulated here by wiping sales) must not badge anything.
        crate::db::schema::apply(&conn).unwrap();
        conn.execute("DELETE FROM sale_items", []).unwrap();
        conn.execute("DELETE FROM sales", []).unwrap();

        let ids = get_best_selling_item_ids(&conn, 30, 5).unwrap();
        assert!(ids.is_empty(), "no sales history must mean no best-seller badges");
    }

    #[test]
    fn best_sellers_respects_the_limit() {
        let conn = test_conn();
        let ids = get_best_selling_item_ids(&conn, 30, 1).unwrap();
        assert!(ids.len() <= 1);
    }

    /// Rings up a fresh Cola x2 sale and refunds 1 unit of it, with a reason
    /// and a `refunded_by` set — a clean, known refund every test below
    /// checks the report against.
    fn seed_one_refund(conn: &mut Connection) -> i64 {
        use crate::db::refunds::{create_refund, CreateRefundInput, RefundLineInput};
        use crate::db::sales::{create_sale, CartLine, CreateSaleInput};

        let tx = conn.transaction().unwrap();
        let cola: i64 =
            tx.query_row("SELECT id FROM items WHERE name = 'Cola 500ml'", [], |row| row.get(0)).unwrap();
        let owner_id: i64 = tx.query_row("SELECT id FROM users LIMIT 1", [], |row| row.get(0)).unwrap();
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
        let sale_item_id: i64 =
            tx.query_row("SELECT id FROM sale_items WHERE sale_id = ?1", params![sale.id], |row| row.get(0)).unwrap();
        let refund = create_refund(
            &tx,
            CreateRefundInput {
                sale_id: sale.id,
                items: vec![RefundLineInput { sale_item_id, qty: 1, amount_minor: 8000 }],
                reason: Some("Customer changed mind".into()),
                refunded_by: Some(owner_id),
            },
        )
        .unwrap();
        tx.commit().unwrap();
        refund.id
    }

    #[test]
    fn refunds_summary_lists_a_refund_with_its_item_reason_and_processed_by() {
        let mut conn = test_conn();
        let refund_id = seed_one_refund(&mut conn);

        let report = get_refunds_summary(&conn, &days_ago(30), &days_ago(0)).unwrap();
        let refund = report.refunds.iter().find(|r| r.id == refund_id).expect("seeded refund must appear");

        assert_eq!(refund.items.len(), 1);
        assert_eq!(refund.items[0].item_name, "Cola 500ml");
        assert_eq!(refund.items[0].qty_refunded, 1);
        assert_eq!(refund.reason.as_deref(), Some("Customer changed mind"));
        assert!(refund.refunded_by_name.is_some(), "refunded_by must resolve to a user name");
        assert_eq!(refund.total_refund_amount_minor, 8000);
    }

    #[test]
    fn refunds_summary_grand_total_matches_the_sum_of_every_row() {
        let mut conn = test_conn();
        seed_one_refund(&mut conn);
        seed_one_refund(&mut conn);

        let report = get_refunds_summary(&conn, &days_ago(30), &days_ago(0)).unwrap();
        let rows_sum: i64 = report.refunds.iter().map(|r| r.total_refund_amount_minor).sum();
        assert_eq!(report.grand_total_refunded_minor, rows_sum);
        assert!(report.refunds.len() >= 2, "both seeded refunds must appear");
    }

    #[test]
    fn refunds_summary_sorts_most_recent_first() {
        let mut conn = test_conn();
        seed_one_refund(&mut conn);
        seed_one_refund(&mut conn);

        let report = get_refunds_summary(&conn, &days_ago(30), &days_ago(0)).unwrap();
        for pair in report.refunds.windows(2) {
            assert!(pair[0].id >= pair[1].id, "most recently created refund must come first");
        }
    }

    #[test]
    fn refunds_summary_is_empty_for_a_range_with_no_refunds() {
        let conn = test_conn(); // seed data has sales but no refunds
        let report = get_refunds_summary(&conn, &days_ago(30), &days_ago(0)).unwrap();
        assert!(report.refunds.is_empty());
        assert_eq!(report.grand_total_refunded_minor, 0);
    }
}
