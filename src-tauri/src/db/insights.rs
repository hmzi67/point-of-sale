//! Profit, payment, tax, void and inventory reports, plus the ledger
//! dashboard summary. Read-only, like `db::reports`, and every range uses
//! `reports::validate_range`'s bounds and limit.
//!
//! Cost of goods uses `sale_items.cost_at_sale_minor` — the item's cost at
//! checkout — falling back to the item's current cost for lines sold before
//! that column existed.

use std::collections::BTreeMap;

use rusqlite::{params, Connection};
use serde::Serialize;

use crate::db::reports::{validate_range, ReportError};
use crate::db::{customers, dates, purchasing};

/// Per-line cost of goods, in minor units.
const LINE_COST_SQL: &str = "ROUND(si.qty * COALESCE(si.cost_at_sale_minor, i.cost_minor))";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodRow {
    /// `YYYY-MM-DD`, `YYYY-Www` or `YYYY-MM`, per the requested grouping.
    pub period: String,
    pub transactions: i64,
    pub net_revenue_minor: i64,
    pub cogs_minor: i64,
    pub gross_profit_minor: i64,
    pub expenses_minor: i64,
    pub net_profit_minor: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfitLoss {
    pub start_date: String,
    pub end_date: String,
    /// Item subtotals before discounts.
    pub gross_sales_minor: i64,
    pub discounts_minor: i64,
    pub refunds_minor: i64,
    pub service_charge_minor: i64,
    /// `gross - discounts - refunds + service charge`. Tax is collected for
    /// the government, so it is reported (`tax_minor`) but not revenue.
    pub net_revenue_minor: i64,
    pub tax_minor: i64,
    /// Cost of goods sold, less the cost of goods returned.
    pub cogs_minor: i64,
    pub gross_profit_minor: i64,
    /// `None` when the module is off (not tracked), as on the dashboard.
    pub expenses_minor: Option<i64>,
    pub salaries_minor: Option<i64>,
    pub net_profit_minor: i64,
    pub transactions: i64,
    pub periods: Vec<PeriodRow>,
}

fn period_expr(column: &str, grouping: &str) -> String {
    match grouping {
        "week" => format!("strftime('%Y-W%W', {column})"),
        "month" => format!("strftime('%Y-%m', {column})"),
        _ => format!("date({column})"),
    }
}

#[derive(Default)]
struct PeriodAcc {
    transactions: i64,
    revenue: i64,
    cogs: i64,
    expenses: i64,
}

pub fn profit_loss(
    conn: &Connection,
    start_date: &str,
    end_date: &str,
    grouping: &str,
    include_expenses: bool,
    include_salaries: bool,
) -> Result<ProfitLoss, ReportError> {
    let (start_ts, end_ts) = validate_range(start_date, end_date)?;
    let mut periods: BTreeMap<String, PeriodAcc> = BTreeMap::new();

    // Sales: subtotal = total - tax - service + discount.
    let mut stmt = conn.prepare(&format!(
        "SELECT {}, COUNT(*), COALESCE(SUM(total_minor - tax_minor - service_charge_minor + discount_minor), 0),
                COALESCE(SUM(discount_minor), 0), COALESCE(SUM(service_charge_minor), 0), COALESCE(SUM(tax_minor), 0)
           FROM sales WHERE created_at >= ?1 AND created_at <= ?2
          GROUP BY 1",
        period_expr("created_at", grouping)
    ))?;
    let mut gross = 0;
    let mut discounts = 0;
    let mut service = 0;
    let mut tax = 0;
    let mut transactions = 0;
    for row in stmt.query_map(params![start_ts, end_ts], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, i64>(5)?,
        ))
    })? {
        let (period, count, subtotal, discount, service_charge, tax_minor) = row?;
        gross += subtotal;
        discounts += discount;
        service += service_charge;
        tax += tax_minor;
        transactions += count;
        let acc = periods.entry(period).or_default();
        acc.transactions += count;
        acc.revenue += subtotal - discount + service_charge;
    }

    let mut stmt = conn.prepare(&format!(
        "SELECT {}, COALESCE(SUM({LINE_COST_SQL}), 0)
           FROM sale_items si JOIN sales s ON s.id = si.sale_id JOIN items i ON i.id = si.item_id
          WHERE s.created_at >= ?1 AND s.created_at <= ?2
          GROUP BY 1",
        period_expr("s.created_at", grouping)
    ))?;
    let mut cogs = 0;
    for row in stmt.query_map(params![start_ts, end_ts], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
    })? {
        let (period, cost) = row?;
        cogs += cost as i64;
        periods.entry(period).or_default().cogs += cost as i64;
    }

    // Refunds by when they happened: revenue out, and the goods' cost back.
    let mut stmt = conn.prepare(&format!(
        "SELECT {}, COALESCE(SUM(ri.amount_refunded_minor), 0),
                COALESCE(SUM(ROUND(ri.qty_refunded * COALESCE(si.cost_at_sale_minor, i.cost_minor))), 0)
           FROM refund_items ri
           JOIN refunds r ON r.id = ri.refund_id
           JOIN sale_items si ON si.id = ri.sale_item_id
           JOIN items i ON i.id = si.item_id
          WHERE r.created_at >= ?1 AND r.created_at <= ?2
          GROUP BY 1",
        period_expr("r.created_at", grouping)
    ))?;
    let mut refunds = 0;
    for row in stmt.query_map(params![start_ts, end_ts], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, f64>(2)?,
        ))
    })? {
        let (period, amount, cost) = row?;
        refunds += amount;
        cogs -= cost as i64;
        let acc = periods.entry(period).or_default();
        acc.revenue -= amount;
        acc.cogs -= cost as i64;
    }

    let mut expenses_minor = None;
    if include_expenses {
        let mut total = 0;
        let mut stmt = conn.prepare(&format!(
            "SELECT {}, COALESCE(SUM(amount_minor), 0) FROM expenses
              WHERE expense_date BETWEEN ?1 AND ?2 GROUP BY 1",
            period_expr("expense_date", grouping)
        ))?;
        for row in stmt.query_map(params![start_date, end_date], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })? {
            let (period, amount) = row?;
            total += amount;
            periods.entry(period).or_default().expenses += amount;
        }
        expenses_minor = Some(total);
    }
    let mut salaries_minor = None;
    if include_salaries {
        let mut total = 0;
        let mut stmt = conn.prepare(&format!(
            "SELECT {}, COALESCE(SUM(paid_amount_minor), 0) FROM salary_payments
              WHERE paid_date BETWEEN ?1 AND ?2 GROUP BY 1",
            period_expr("paid_date", grouping)
        ))?;
        for row in stmt.query_map(params![start_date, end_date], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })? {
            let (period, amount) = row?;
            total += amount;
            periods.entry(period).or_default().expenses += amount;
        }
        salaries_minor = Some(total);
    }

    let net_revenue_minor = gross - discounts - refunds + service;
    let gross_profit_minor = net_revenue_minor - cogs;
    let net_profit_minor =
        gross_profit_minor - expenses_minor.unwrap_or(0) - salaries_minor.unwrap_or(0);
    let periods = periods
        .into_iter()
        .map(|(period, acc)| PeriodRow {
            period,
            transactions: acc.transactions,
            net_revenue_minor: acc.revenue,
            cogs_minor: acc.cogs,
            gross_profit_minor: acc.revenue - acc.cogs,
            expenses_minor: acc.expenses,
            net_profit_minor: acc.revenue - acc.cogs - acc.expenses,
        })
        .collect();

    Ok(ProfitLoss {
        start_date: start_date.to_string(),
        end_date: end_date.to_string(),
        gross_sales_minor: gross,
        discounts_minor: discounts,
        refunds_minor: refunds,
        service_charge_minor: service,
        net_revenue_minor,
        tax_minor: tax,
        cogs_minor: cogs,
        gross_profit_minor,
        expenses_minor,
        salaries_minor,
        net_profit_minor,
        transactions,
        periods,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderProfitRow {
    pub sale_id: i64,
    pub order_no: Option<i64>,
    pub created_at: String,
    pub cashier_name: Option<String>,
    pub customer_name: Option<String>,
    pub payment_method: String,
    pub item_count: f64,
    /// Subtotal - discount + service charge (tax excluded).
    pub revenue_minor: i64,
    pub cost_minor: i64,
    pub refunded_minor: i64,
    pub profit_minor: i64,
    pub margin_percent: f64,
}

/// Profit per sale, newest first (capped at 2000 rows).
pub fn order_profit(
    conn: &Connection,
    start_date: &str,
    end_date: &str,
) -> Result<Vec<OrderProfitRow>, ReportError> {
    let (start_ts, end_ts) = validate_range(start_date, end_date)?;
    let mut stmt = conn.prepare(&format!(
        "SELECT s.id, o.order_no, s.created_at, u.name, COALESCE(c.name, o.customer_name), s.payment_method,
                (SELECT COALESCE(SUM(si.qty), 0) FROM sale_items si WHERE si.sale_id = s.id),
                s.total_minor - s.tax_minor,
                (SELECT COALESCE(SUM({LINE_COST_SQL}), 0) FROM sale_items si JOIN items i ON i.id = si.item_id
                  WHERE si.sale_id = s.id),
                (SELECT COALESCE(SUM(total_refund_amount_minor), 0) FROM refunds WHERE original_sale_id = s.id),
                (SELECT COALESCE(SUM(ROUND(ri.qty_refunded * COALESCE(si.cost_at_sale_minor, i.cost_minor))), 0)
                   FROM refund_items ri JOIN sale_items si ON si.id = ri.sale_item_id JOIN items i ON i.id = si.item_id
                  WHERE si.sale_id = s.id)
           FROM sales s
           LEFT JOIN users u ON u.id = s.cashier_id
           LEFT JOIN orders o ON o.id = s.order_id
           LEFT JOIN customers c ON c.id = s.customer_id
          WHERE s.created_at >= ?1 AND s.created_at <= ?2
          ORDER BY s.id DESC
          LIMIT 2000"
    ))?;
    let rows = stmt.query_map(params![start_ts, end_ts], |row| {
        let revenue: i64 = row.get(7)?;
        let cost = row.get::<_, f64>(8)? as i64;
        let refunded: i64 = row.get(9)?;
        let refunded_cost = row.get::<_, f64>(10)? as i64;
        let profit = revenue - refunded - (cost - refunded_cost);
        let net = revenue - refunded;
        Ok(OrderProfitRow {
            sale_id: row.get(0)?,
            order_no: row.get(1)?,
            created_at: row.get(2)?,
            cashier_name: row.get(3)?,
            customer_name: row.get(4)?,
            payment_method: row.get(5)?,
            item_count: row.get(6)?,
            revenue_minor: revenue,
            cost_minor: cost - refunded_cost,
            refunded_minor: refunded,
            profit_minor: profit,
            margin_percent: if net > 0 {
                (profit as f64 / net as f64 * 1000.0).round() / 10.0
            } else {
                0.0
            },
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentTypeRow {
    pub method: String,
    pub amount_minor: i64,
    pub sale_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentTypeReport {
    pub rows: Vec<PaymentTypeRow>,
    /// Pay-later installments collected in the range, by method.
    pub collections: Vec<PaymentTypeRow>,
    pub refunds_minor: i64,
    pub split_sale_count: i64,
}

pub fn payment_types(
    conn: &Connection,
    start_date: &str,
    end_date: &str,
) -> Result<PaymentTypeReport, ReportError> {
    let (start_ts, end_ts) = validate_range(start_date, end_date)?;
    let collect = |sql: &str| -> Result<Vec<PaymentTypeRow>, rusqlite::Error> {
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![start_ts, end_ts], |row| {
            Ok(PaymentTypeRow {
                method: row.get(0)?,
                amount_minor: row.get(1)?,
                sale_count: row.get(2)?,
            })
        })?;
        rows.collect()
    };
    let rows = collect(
        "SELECT p.method, SUM(p.amount_minor), COUNT(DISTINCT p.sale_id)
           FROM sale_payment_lines p JOIN sales s ON s.id = p.sale_id
          WHERE s.created_at >= ?1 AND s.created_at <= ?2
          GROUP BY p.method ORDER BY 2 DESC",
    )?;
    let collections = collect(
        "SELECT method, SUM(amount_minor), COUNT(DISTINCT sale_id) FROM customer_payments
          WHERE created_at >= ?1 AND created_at <= ?2
          GROUP BY method ORDER BY 2 DESC",
    )?;
    let refunds_minor = conn.query_row(
        "SELECT COALESCE(SUM(total_refund_amount_minor), 0) FROM refunds WHERE created_at >= ?1 AND created_at <= ?2",
        params![start_ts, end_ts],
        |row| row.get(0),
    )?;
    let split_sale_count = conn.query_row(
        "SELECT COUNT(*) FROM sales WHERE payment_method = 'split' AND created_at >= ?1 AND created_at <= ?2",
        params![start_ts, end_ts],
        |row| row.get(0),
    )?;
    Ok(PaymentTypeReport {
        rows,
        collections,
        refunds_minor,
        split_sale_count,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaxDayRow {
    pub date: String,
    pub transactions: i64,
    pub taxable_minor: i64,
    pub tax_minor: i64,
    pub service_charge_minor: i64,
    pub refunds_minor: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaxReport {
    pub rows: Vec<TaxDayRow>,
    pub taxable_minor: i64,
    pub tax_minor: i64,
    pub service_charge_minor: i64,
    pub refunds_minor: i64,
    pub tax_exempt_sale_count: i64,
}

pub fn tax_report(
    conn: &Connection,
    start_date: &str,
    end_date: &str,
) -> Result<TaxReport, ReportError> {
    let (start_ts, end_ts) = validate_range(start_date, end_date)?;
    let mut days: BTreeMap<String, TaxDayRow> = BTreeMap::new();
    let mut stmt = conn.prepare(
        "SELECT date(created_at), COUNT(*), SUM(total_minor - tax_minor - service_charge_minor),
                SUM(tax_minor), SUM(service_charge_minor)
           FROM sales WHERE created_at >= ?1 AND created_at <= ?2 GROUP BY 1",
    )?;
    for row in stmt.query_map(params![start_ts, end_ts], |row| {
        Ok(TaxDayRow {
            date: row.get(0)?,
            transactions: row.get(1)?,
            taxable_minor: row.get(2)?,
            tax_minor: row.get(3)?,
            service_charge_minor: row.get(4)?,
            refunds_minor: 0,
        })
    })? {
        let row = row?;
        days.insert(row.date.clone(), row);
    }
    let mut stmt = conn.prepare(
        "SELECT date(created_at), SUM(total_refund_amount_minor) FROM refunds
          WHERE created_at >= ?1 AND created_at <= ?2 GROUP BY 1",
    )?;
    for row in stmt.query_map(params![start_ts, end_ts], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })? {
        let (date, amount) = row?;
        days.entry(date.clone())
            .or_insert(TaxDayRow {
                date,
                transactions: 0,
                taxable_minor: 0,
                tax_minor: 0,
                service_charge_minor: 0,
                refunds_minor: 0,
            })
            .refunds_minor += amount;
    }
    let tax_exempt_sale_count = conn.query_row(
        "SELECT COUNT(*) FROM sales WHERE tax_minor = 0 AND created_at >= ?1 AND created_at <= ?2",
        params![start_ts, end_ts],
        |row| row.get(0),
    )?;
    let rows: Vec<TaxDayRow> = days.into_values().collect();
    Ok(TaxReport {
        taxable_minor: rows.iter().map(|r| r.taxable_minor).sum(),
        tax_minor: rows.iter().map(|r| r.tax_minor).sum(),
        service_charge_minor: rows.iter().map(|r| r.service_charge_minor).sum(),
        refunds_minor: rows.iter().map(|r| r.refunds_minor).sum(),
        tax_exempt_sale_count,
        rows,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoidRow {
    pub order_id: i64,
    pub order_no: i64,
    pub item_name: String,
    pub qty: f64,
    pub value_minor: i64,
    pub reason: Option<String>,
    pub voided_by: Option<String>,
    pub voided_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelledOrderRow {
    pub order_id: i64,
    pub order_no: i64,
    pub order_type: String,
    pub reason: Option<String>,
    pub cancelled_at: String,
    pub value_minor: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoidReport {
    pub voids: Vec<VoidRow>,
    pub cancelled_orders: Vec<CancelledOrderRow>,
    pub void_value_minor: i64,
    pub cancelled_value_minor: i64,
    pub refunds_minor: i64,
    pub refund_count: i64,
}

pub fn void_report(
    conn: &Connection,
    start_date: &str,
    end_date: &str,
) -> Result<VoidReport, ReportError> {
    let (start_ts, end_ts) = validate_range(start_date, end_date)?;
    let mut stmt = conn.prepare(
        "SELECT o.id, o.order_no, i.name, oi.qty, ROUND(oi.qty * oi.price_at_order_minor), oi.void_reason,
                u.name, oi.voided_at
           FROM order_items oi
           JOIN orders o ON o.id = oi.order_id
           JOIN items i ON i.id = oi.item_id
           LEFT JOIN users u ON u.id = oi.voided_by
          WHERE oi.status = 'voided' AND oi.voided_at >= ?1 AND oi.voided_at <= ?2
            AND o.status != 'cancelled'
          ORDER BY oi.voided_at DESC",
    )?;
    let voids = stmt
        .query_map(params![start_ts, end_ts], |row| {
            Ok(VoidRow {
                order_id: row.get(0)?,
                order_no: row.get(1)?,
                item_name: row.get(2)?,
                qty: row.get(3)?,
                value_minor: row.get::<_, f64>(4)? as i64,
                reason: row.get(5)?,
                voided_by: row.get(6)?,
                voided_at: row.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut stmt = conn.prepare(
        "SELECT o.id, o.order_no, o.order_type, o.cancel_reason, o.cancelled_at,
                (SELECT COALESCE(SUM(ROUND(qty * price_at_order_minor)), 0) FROM order_items WHERE order_id = o.id)
           FROM orders o
          WHERE o.status = 'cancelled' AND o.cancelled_at >= ?1 AND o.cancelled_at <= ?2
          ORDER BY o.cancelled_at DESC",
    )?;
    let cancelled_orders = stmt
        .query_map(params![start_ts, end_ts], |row| {
            Ok(CancelledOrderRow {
                order_id: row.get(0)?,
                order_no: row.get(1)?,
                order_type: row.get(2)?,
                reason: row.get(3)?,
                cancelled_at: row.get(4)?,
                value_minor: row.get::<_, f64>(5)? as i64,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let (refunds_minor, refund_count) = conn.query_row(
        "SELECT COALESCE(SUM(total_refund_amount_minor), 0), COUNT(*) FROM refunds
          WHERE created_at >= ?1 AND created_at <= ?2",
        params![start_ts, end_ts],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok(VoidReport {
        void_value_minor: voids.iter().map(|v| v.value_minor).sum(),
        cancelled_value_minor: cancelled_orders.iter().map(|c| c.value_minor).sum(),
        voids,
        cancelled_orders,
        refunds_minor,
        refund_count,
    })
}

// ---------------------------------------------------------------------------
// Inventory
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StockLevelRow {
    pub item_id: i64,
    pub item_name: String,
    pub category_name: Option<String>,
    pub unit: Option<String>,
    pub stock_qty: f64,
    pub low_stock_threshold: i64,
    pub cost_minor: i64,
    pub price_minor: i64,
    pub cost_value_minor: i64,
    pub retail_value_minor: i64,
    pub is_low_stock: bool,
    /// Ordered on an open purchase order and not yet received.
    pub incoming_qty: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StockLevelReport {
    pub rows: Vec<StockLevelRow>,
    pub cost_value_minor: i64,
    pub retail_value_minor: i64,
    pub low_stock_count: i64,
    pub out_of_stock_count: i64,
}

/// Every active item's stock, its value at cost and at retail, and whether
/// it is at or under its low-stock threshold.
pub fn stock_levels(conn: &Connection) -> Result<StockLevelReport, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT i.id, i.name, c.name, i.unit, i.stock_qty, i.low_stock_threshold, i.cost_minor, i.price_minor,
                (SELECT COALESCE(SUM(l.qty_ordered - l.qty_received), 0)
                   FROM purchase_order_items l JOIN purchase_orders p ON p.id = l.po_id
                  WHERE l.item_id = i.id AND p.status IN ('ordered', 'partial') AND l.qty_received < l.qty_ordered)
           FROM items i LEFT JOIN categories c ON c.id = i.category_id
          WHERE i.is_active = 1
          ORDER BY (i.stock_qty <= i.low_stock_threshold) DESC, i.name COLLATE NOCASE",
    )?;
    let rows = stmt
        .query_map([], |row| {
            let stock_qty: f64 = row.get(4)?;
            let threshold: i64 = row.get(5)?;
            let cost: i64 = row.get(6)?;
            let price: i64 = row.get(7)?;
            Ok(StockLevelRow {
                item_id: row.get(0)?,
                item_name: row.get(1)?,
                category_name: row.get(2)?,
                unit: row.get(3)?,
                stock_qty,
                low_stock_threshold: threshold,
                cost_minor: cost,
                price_minor: price,
                cost_value_minor: (stock_qty.max(0.0) * cost as f64).round() as i64,
                retail_value_minor: (stock_qty.max(0.0) * price as f64).round() as i64,
                is_low_stock: stock_qty <= threshold as f64,
                incoming_qty: row.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(StockLevelReport {
        cost_value_minor: rows.iter().map(|r| r.cost_value_minor).sum(),
        retail_value_minor: rows.iter().map(|r| r.retail_value_minor).sum(),
        low_stock_count: rows.iter().filter(|r| r.is_low_stock).count() as i64,
        out_of_stock_count: rows.iter().filter(|r| r.stock_qty <= 0.0).count() as i64,
        rows,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRow {
    pub item_id: i64,
    pub item_name: String,
    pub unit: Option<String>,
    pub opening_qty: f64,
    pub received_qty: f64,
    pub sold_qty: f64,
    pub returned_qty: f64,
    pub damaged_qty: f64,
    /// Manual additions, corrections and stock-take variances (signed).
    pub adjusted_qty: f64,
    pub closing_qty: f64,
    /// Sold ÷ average of opening and closing stock.
    pub turnover: f64,
    /// Days the closing stock lasts at this period's selling rate; `None`
    /// when nothing sold.
    pub days_of_cover: Option<f64>,
    pub cogs_minor: i64,
}

#[derive(Default, Clone, Copy)]
struct Flow {
    received: f64,
    sold: f64,
    returned: f64,
    damaged: f64,
    adjusted: f64,
}

impl Flow {
    fn net(&self) -> f64 {
        self.received - self.sold + self.returned - self.damaged + self.adjusted
    }
}

fn flows(
    conn: &Connection,
    from_ts: &str,
    to_ts: &str,
) -> Result<BTreeMap<i64, Flow>, rusqlite::Error> {
    let mut map: BTreeMap<i64, Flow> = BTreeMap::new();
    let mut stmt = conn.prepare(
        "SELECT item_id, reason, SUM(qty_change) FROM stock_movements
          WHERE created_at >= ?1 AND created_at <= ?2 GROUP BY item_id, reason",
    )?;
    for row in stmt.query_map(params![from_ts, to_ts], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, f64>(2)?,
        ))
    })? {
        let (item, reason, qty) = row?;
        let flow = map.entry(item).or_default();
        match reason.as_str() {
            "grn" => flow.received += qty,
            "damage" => flow.damaged -= qty,
            _ => flow.adjusted += qty,
        }
    }
    let mut stmt = conn.prepare(
        "SELECT si.item_id, SUM(si.qty) FROM sale_items si JOIN sales s ON s.id = si.sale_id
          WHERE s.created_at >= ?1 AND s.created_at <= ?2 GROUP BY si.item_id",
    )?;
    for row in stmt.query_map(params![from_ts, to_ts], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, f64>(1)?))
    })? {
        let (item, qty) = row?;
        map.entry(item).or_default().sold += qty;
    }
    let mut stmt = conn.prepare(
        "SELECT si.item_id, SUM(ri.qty_refunded) FROM refund_items ri
           JOIN refunds r ON r.id = ri.refund_id JOIN sale_items si ON si.id = ri.sale_item_id
          WHERE r.created_at >= ?1 AND r.created_at <= ?2 GROUP BY si.item_id",
    )?;
    for row in stmt.query_map(params![from_ts, to_ts], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, f64>(1)?))
    })? {
        let (item, qty) = row?;
        map.entry(item).or_default().returned += qty;
    }
    Ok(map)
}

/// Stock flow per item over the range — what came in, went out, and how
/// fast it turns. Opening/closing are reconstructed backwards from today's
/// stock through every recorded movement, sale and refund.
pub fn inventory_usage(
    conn: &Connection,
    start_date: &str,
    end_date: &str,
) -> Result<Vec<UsageRow>, ReportError> {
    let (start_ts, end_ts) = validate_range(start_date, end_date)?;
    let (start, end) = crate::db::dates::parse_date_range(start_date, end_date)
        .map_err(ReportError::InvalidDateRange)?;
    let days = ((end - start).num_days() + 1) as f64;
    let in_range = flows(conn, &start_ts, &end_ts)?;
    // Everything after the range: ":60" sorts after any real end-of-day
    // timestamp and before the next day's, so the bound is exclusive.
    let after = flows(
        conn,
        &format!("{end_date} 23:59:60"),
        dates::OPEN_END_TIMESTAMP,
    )?;

    let mut stmt = conn.prepare(
        "SELECT id, name, unit, stock_qty, cost_minor FROM items WHERE is_active = 1 ORDER BY name COLLATE NOCASE",
    )?;
    let items = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, f64>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let round = |q: f64| (q * 1000.0).round() / 1000.0;
    let mut rows = Vec::with_capacity(items.len());
    for (id, name, unit, stock_now, cost) in items {
        let flow = in_range.get(&id).copied().unwrap_or_default();
        let later = after.get(&id).copied().unwrap_or_default();
        let closing = stock_now - later.net();
        let opening = closing - flow.net();
        let average = (opening + closing) / 2.0;
        let rate = flow.sold / days;
        rows.push(UsageRow {
            item_id: id,
            item_name: name,
            unit,
            opening_qty: round(opening),
            received_qty: round(flow.received),
            sold_qty: round(flow.sold),
            returned_qty: round(flow.returned),
            damaged_qty: round(flow.damaged),
            adjusted_qty: round(flow.adjusted),
            closing_qty: round(closing),
            turnover: if average > 0.0 {
                (flow.sold / average * 100.0).round() / 100.0
            } else {
                0.0
            },
            days_of_cover: if rate > 0.0 {
                Some((closing.max(0.0) / rate * 10.0).round() / 10.0)
            } else {
                None
            },
            cogs_minor: ((flow.sold - flow.returned) * cost as f64).round() as i64,
        });
    }
    rows.sort_by(|a, b| {
        b.sold_qty
            .partial_cmp(&a.sold_qty)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(rows)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpiryRow {
    pub item_id: i64,
    pub item_name: String,
    pub batch_no: Option<String>,
    pub expiry_date: String,
    /// Negative once expired.
    pub days_left: i64,
    pub qty_received: f64,
    pub stock_qty: f64,
    pub supplier_name: String,
    pub grn_id: i64,
    pub received_date: String,
}

/// Received batches expiring within `within_days` (or already expired) for
/// items still in stock, soonest first.
pub fn expiry_report(
    conn: &Connection,
    within_days: i64,
) -> Result<Vec<ExpiryRow>, rusqlite::Error> {
    let today = chrono::Local::now().date_naive();
    let limit = (today + chrono::Duration::days(within_days.clamp(0, 3650)))
        .format("%Y-%m-%d")
        .to_string();
    let mut stmt = conn.prepare(
        "SELECT l.item_id, i.name, l.batch_no, l.expiry_date, l.qty, i.stock_qty, s.name, g.id, g.received_date
           FROM goods_receipt_items l
           JOIN goods_receipts g ON g.id = l.grn_id
           JOIN suppliers s ON s.id = g.supplier_id
           JOIN items i ON i.id = l.item_id
          WHERE l.expiry_date IS NOT NULL AND l.expiry_date <= ?1 AND i.stock_qty > 0 AND i.is_active = 1
          ORDER BY l.expiry_date, i.name",
    )?;
    let rows = stmt.query_map(params![limit], |row| {
        let expiry: String = row.get(3)?;
        let days_left = crate::db::dates::parse_date(&expiry)
            .map(|d| (d - today).num_days())
            .unwrap_or(0);
        Ok(ExpiryRow {
            item_id: row.get(0)?,
            item_name: row.get(1)?,
            batch_no: row.get(2)?,
            expiry_date: expiry,
            days_left,
            qty_received: row.get(4)?,
            stock_qty: row.get(5)?,
            supplier_name: row.get(6)?,
            grn_id: row.get(7)?,
            received_date: row.get(8)?,
        })
    })?;
    rows.collect()
}

// ---------------------------------------------------------------------------
// Ledger dashboard
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerParty {
    pub id: i64,
    pub name: String,
    pub phone: Option<String>,
    pub balance_minor: i64,
    pub overdue_minor: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerSummary {
    pub receivable_minor: i64,
    pub overdue_minor: i64,
    pub overdue: customers::OverdueSummary,
    pub payable_minor: i64,
    pub collected_this_month_minor: i64,
    pub paid_out_this_month_minor: i64,
    pub debtors: Vec<LedgerParty>,
    pub creditors: Vec<LedgerParty>,
}

pub fn ledger_summary(conn: &Connection) -> Result<LedgerSummary, rusqlite::Error> {
    let mut debtors: Vec<LedgerParty> = customers::list_customers(conn, None, true, 5000)?
        .into_iter()
        .filter(|c| c.balance_minor > 0)
        .map(|c| LedgerParty {
            id: c.id,
            name: c.name,
            phone: c.phone,
            balance_minor: c.balance_minor,
            overdue_minor: c.overdue_minor,
        })
        .collect();
    debtors.sort_by_key(|party| std::cmp::Reverse(party.balance_minor));
    let mut creditors: Vec<LedgerParty> = purchasing::list_suppliers(conn, true)?
        .into_iter()
        .filter(|s| s.balance_minor > 0)
        .map(|s| LedgerParty {
            id: s.id,
            name: s.name,
            phone: s.phone,
            balance_minor: s.balance_minor,
            overdue_minor: 0,
        })
        .collect();
    creditors.sort_by_key(|party| std::cmp::Reverse(party.balance_minor));

    let month_start = chrono::Local::now()
        .date_naive()
        .format("%Y-%m-01")
        .to_string();
    let collected_this_month_minor = conn.query_row(
        "SELECT COALESCE(SUM(amount_minor), 0) FROM customer_payments WHERE created_at >= ?1",
        params![dates::day_start(&month_start)],
        |row| row.get(0),
    )?;
    let paid_out_this_month_minor = conn.query_row(
        "SELECT COALESCE(SUM(amount_minor), 0) FROM supplier_payments WHERE paid_date >= ?1",
        params![month_start],
        |row| row.get(0),
    )?;

    Ok(LedgerSummary {
        receivable_minor: debtors.iter().map(|d| d.balance_minor).sum(),
        overdue_minor: debtors.iter().map(|d| d.overdue_minor).sum(),
        overdue: customers::overdue_summary(conn)?,
        payable_minor: creditors.iter().map(|c| c.balance_minor).sum(),
        collected_this_month_minor,
        paid_out_this_month_minor,
        debtors,
        creditors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::customers::{add_customer, CustomerInput};
    use crate::db::purchasing::{
        add_supplier, receive_goods, GrnInput, GrnLineInput, SupplierInput,
    };
    use crate::db::refunds::{create_refund, CreateRefundInput, RefundLineInput};
    use crate::db::sales::{create_sale, CreateSaleInput, PaymentPart};
    use crate::db::schema::test_conn;
    use crate::db::test_support::{
        backdate_sale, in_tx, item_id, one_line_sale, ring_up, ring_up_with_line,
    };

    /// A fixed day the seed never touches — no "today" that a run crossing
    /// midnight could split.
    const DAY: &str = "2031-08-10";

    fn at_noon() -> String {
        format!("{DAY} 12:00:00")
    }

    fn price_and_cost(conn: &Connection, name: &str) -> (i64, i64) {
        conn.query_row(
            "SELECT price_minor, cost_minor FROM items WHERE name = ?1",
            [name],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
    }

    #[test]
    fn profit_and_loss_subtracts_cost_of_goods_and_returns() {
        let mut conn = test_conn();
        let (price, cost) = price_and_cost(&conn, "Cola 500ml");
        let (sale, line) = ring_up_with_line(&mut conn, "Cola 500ml", 3.0);
        backdate_sale(&conn, sale, &at_noon());
        let refund = in_tx(&mut conn, |tx| {
            create_refund(
                tx,
                CreateRefundInput {
                    sale_id: sale,
                    refunded_by: None,
                    reason: None,
                    items: vec![RefundLineInput {
                        sale_item_id: line,
                        qty: 1.0,
                        amount_minor: price,
                    }],
                },
            )
        })
        .unwrap();
        conn.execute(
            "UPDATE refunds SET created_at = ?1 WHERE id = ?2",
            params![at_noon(), refund.id],
        )
        .unwrap();

        let pl = profit_loss(&conn, DAY, DAY, "day", false, false).unwrap();
        assert_eq!(pl.transactions, 1);
        assert_eq!(pl.gross_sales_minor, 3 * price);
        assert_eq!(pl.refunds_minor, price);
        assert_eq!(pl.net_revenue_minor, 2 * price);
        assert_eq!(
            pl.cogs_minor,
            2 * cost,
            "the returned unit's cost comes back off"
        );
        assert_eq!(pl.gross_profit_minor, 2 * (price - cost));
        assert_eq!(
            pl.net_profit_minor, pl.gross_profit_minor,
            "expenses/salaries not tracked here"
        );
        assert_eq!(pl.periods.len(), 1);
        assert_eq!(pl.periods[0].period, DAY);
        assert_eq!(pl.periods[0].gross_profit_minor, 2 * (price - cost));

        let monthly = profit_loss(&conn, DAY, DAY, "month", false, false).unwrap();
        assert_eq!(monthly.periods[0].period, "2031-08");
    }

    #[test]
    fn order_profit_reports_margin_per_sale() {
        let mut conn = test_conn();
        let (price, cost) = price_and_cost(&conn, "Cola 500ml");
        let sale = ring_up(&mut conn, "Cola 500ml", 2.0, "card", None);
        backdate_sale(&conn, sale, &at_noon());

        let rows = order_profit(&conn, DAY, DAY).unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.sale_id, sale);
        assert_eq!(row.revenue_minor, 2 * price);
        assert_eq!(row.cost_minor, 2 * cost);
        assert_eq!(row.profit_minor, 2 * (price - cost));
        let margin = ((price - cost) as f64 / price as f64 * 1000.0).round() / 10.0;
        assert_eq!(row.margin_percent, margin);
    }

    #[test]
    fn payment_types_count_each_part_of_a_split_bill() {
        let mut conn = test_conn();
        let cola = item_id(&conn, "Cola 500ml");
        let (price, _) = price_and_cost(&conn, "Cola 500ml");
        let sale = in_tx(&mut conn, |tx| {
            create_sale(
                tx,
                CreateSaleInput {
                    payments: vec![
                        PaymentPart {
                            method: "cash".into(),
                            amount_minor: 1000,
                        },
                        PaymentPart {
                            method: "card".into(),
                            amount_minor: price - 1000,
                        },
                    ],
                    ..one_line_sale(cola, 1.0)
                },
            )
        })
        .unwrap();
        backdate_sale(&conn, sale.id, &at_noon());

        let report = payment_types(&conn, DAY, DAY).unwrap();
        let amount = |method: &str| {
            report
                .rows
                .iter()
                .find(|r| r.method == method)
                .map(|r| r.amount_minor)
        };
        assert_eq!(amount("cash"), Some(1000));
        assert_eq!(amount("card"), Some(price - 1000));
        assert_eq!(report.split_sale_count, 1);
        assert!(report.rows.iter().all(|r| r.sale_count == 1));
    }

    #[test]
    fn tax_report_totals_tax_and_service_and_counts_tax_free_sales() {
        let mut conn = test_conn();
        let cola = item_id(&conn, "Cola 500ml");
        let (price, _) = price_and_cost(&conn, "Cola 500ml");
        let taxed = in_tx(&mut conn, |tx| {
            create_sale(
                tx,
                CreateSaleInput {
                    tax_minor: 400,
                    service_charge_minor: 250,
                    ..one_line_sale(cola, 1.0)
                },
            )
        })
        .unwrap();
        backdate_sale(&conn, taxed.id, &at_noon());
        let untaxed = ring_up(&mut conn, "Cola 500ml", 1.0, "cash", None);
        backdate_sale(&conn, untaxed, &at_noon());

        let report = tax_report(&conn, DAY, DAY).unwrap();
        assert_eq!(report.rows.len(), 1);
        assert_eq!(report.tax_minor, 400);
        assert_eq!(report.service_charge_minor, 250);
        assert_eq!(report.taxable_minor, 2 * price);
        assert_eq!(report.tax_exempt_sale_count, 1);
    }

    #[test]
    fn void_report_separates_voided_lines_from_cancelled_orders() {
        let conn = test_conn();
        let cola = item_id(&conn, "Cola 500ml");
        conn.execute_batch(&format!(
            "INSERT INTO orders (id, order_no, order_type, status, client_ref, created_at)
                 VALUES (9001, 1, 'takeaway', 'served', 'void-a', '{DAY} 10:00:00'),
                        (9002, 2, 'dine_in', 'cancelled', 'void-b', '{DAY} 10:00:00');
             UPDATE orders SET cancelled_at = '{DAY} 11:00:00', cancel_reason = 'walked out' WHERE id = 9002;
             INSERT INTO order_items (order_id, item_id, qty, price_at_order_minor, status, void_reason, voided_at)
                 VALUES (9001, {cola}, 2, 1500, 'voided', 'wrong item', '{DAY} 10:05:00'),
                        (9001, {cola}, 1, 1500, 'active', NULL, NULL),
                        (9002, {cola}, 4, 1000, 'voided', NULL, '{DAY} 11:00:00');"
        ))
        .unwrap();

        let report = void_report(&conn, DAY, DAY).unwrap();
        assert_eq!(
            report.voids.len(),
            1,
            "a cancelled order's lines are reported with the order, not as voids"
        );
        assert_eq!(report.voids[0].value_minor, 3000);
        assert_eq!(report.voids[0].reason.as_deref(), Some("wrong item"));
        assert_eq!(report.cancelled_orders.len(), 1);
        assert_eq!(report.cancelled_orders[0].value_minor, 4000);
        assert_eq!(report.cancelled_value_minor, 4000);
    }

    #[test]
    fn expiry_report_lists_in_stock_batches_inside_the_window() {
        let mut conn = test_conn();
        let supplier = add_supplier(
            &conn,
            SupplierInput {
                name: "Fresh Co".into(),
                contact_person: None,
                phone: None,
                email: None,
                address: None,
                notes: None,
            },
        )
        .unwrap()
        .id;
        let cola = item_id(&conn, "Cola 500ml");
        let date = |days: i64| {
            (chrono::Local::now().date_naive() + chrono::Duration::days(days))
                .format("%Y-%m-%d")
                .to_string()
        };
        let line = |days: i64, batch: &str| GrnLineInput {
            item_id: cola,
            po_item_id: None,
            qty: 5.0,
            unit_cost_minor: 100,
            batch_no: Some(batch.into()),
            expiry_date: Some(date(days)),
        };
        in_tx(&mut conn, |tx| {
            receive_goods(
                tx,
                GrnInput {
                    supplier_id: supplier,
                    po_id: None,
                    received_date: "2031-01-01".into(),
                    invoice_ref: None,
                    notes: None,
                    lines: vec![line(-3, "OLD"), line(5, "SOON"), line(100, "LATER")],
                },
                None,
            )
        })
        .unwrap();

        let rows = expiry_report(&conn, 30).unwrap();
        let batches: Vec<_> = rows.iter().filter_map(|r| r.batch_no.as_deref()).collect();
        assert_eq!(
            batches,
            ["OLD", "SOON"],
            "soonest first; outside the window is left out"
        );
        assert_eq!(rows[0].days_left, -3);
        assert_eq!(rows[1].days_left, 5);

        conn.execute("UPDATE items SET stock_qty = 0 WHERE id = ?1", [cola])
            .unwrap();
        assert!(
            expiry_report(&conn, 30).unwrap().is_empty(),
            "nothing on the shelf, nothing to flag"
        );
    }

    #[test]
    fn ledger_summary_nets_receivables_against_payables() {
        let mut conn = test_conn();
        let cola = item_id(&conn, "Cola 500ml");
        let (price, _) = price_and_cost(&conn, "Cola 500ml");
        let customer = add_customer(
            &conn,
            CustomerInput {
                name: "Nadia".into(),
                phone: None,
                email: None,
                address: None,
                notes: None,
            },
        )
        .unwrap()
        .id;
        in_tx(&mut conn, |tx| {
            create_sale(
                tx,
                CreateSaleInput {
                    payments: vec![PaymentPart {
                        method: "credit".into(),
                        amount_minor: price,
                    }],
                    customer_id: Some(customer),
                    due_date: Some("2000-01-01".into()),
                    ..one_line_sale(cola, 1.0)
                },
            )
        })
        .unwrap();
        let supplier = add_supplier(
            &conn,
            SupplierInput {
                name: "Metro".into(),
                contact_person: None,
                phone: None,
                email: None,
                address: None,
                notes: None,
            },
        )
        .unwrap()
        .id;
        in_tx(&mut conn, |tx| {
            receive_goods(
                tx,
                GrnInput {
                    supplier_id: supplier,
                    po_id: None,
                    received_date: "2031-01-01".into(),
                    invoice_ref: None,
                    notes: None,
                    lines: vec![GrnLineInput {
                        item_id: cola,
                        po_item_id: None,
                        qty: 10.0,
                        unit_cost_minor: 500,
                        batch_no: None,
                        expiry_date: None,
                    }],
                },
                None,
            )
        })
        .unwrap();

        let summary = ledger_summary(&conn).unwrap();
        assert_eq!(summary.receivable_minor, price);
        assert_eq!(summary.overdue_minor, price);
        assert_eq!(summary.overdue.sale_count, 1);
        assert_eq!(summary.payable_minor, 5000);
        assert_eq!(summary.debtors[0].id, customer);
        assert_eq!(summary.creditors[0].id, supplier);
    }

    #[test]
    fn usage_reconstructs_opening_stock_from_sales_and_movements() {
        let mut conn = test_conn();
        let stock_before: f64 = conn
            .query_row(
                "SELECT stock_qty FROM items WHERE name = 'Cola 500ml'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let sale = ring_up(&mut conn, "Cola 500ml", 3.0, "cash", None);
        backdate_sale(&conn, sale, &at_noon());

        let rows = inventory_usage(&conn, DAY, DAY).unwrap();
        let cola = rows.iter().find(|r| r.item_name == "Cola 500ml").unwrap();
        assert_eq!(cola.sold_qty, 3.0);
        assert_eq!(cola.closing_qty, stock_before - 3.0);
        assert_eq!(cola.opening_qty, stock_before);
        assert!(stock_levels(&conn).unwrap().cost_value_minor > 0);
    }
}
