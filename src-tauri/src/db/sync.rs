//! Database helpers for cloud data synchronization with diwan.codehunts.co.uk.
//!
//! Gathers unsynced sales and recent daily performance summaries from SQLite
//! to push to the customer's cloud dashboard under their server customer/user ID.

use rusqlite::{params, types::ValueRef, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncSalePayload {
    pub local_sale_id: i64,
    pub order_no: Option<i64>,
    pub table_name: Option<String>,
    pub cashier_name: Option<String>,
    pub total_minor: i64,
    pub discount_minor: i64,
    pub tax_minor: i64,
    pub payment_method: String,
    pub currency: String,
    pub items_count: i64,
    pub items_summary: String,
    pub sale_created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncDailySummaryPayload {
    pub summary_date: String,
    pub total_sales_minor: i64,
    pub refunds_minor: i64,
    pub net_sales_minor: i64,
    pub transaction_count: i64,
    pub cash_minor: i64,
    pub card_minor: i64,
    pub other_minor: i64,
    pub currency: String,
}

/// The cloud dashboard's read-only mirror of the POS database. Sensitive
/// local-only tables (users, licence material and sync metadata) are
/// deliberately excluded.
pub fn get_records_for_sync(
    conn: &Connection,
) -> Result<Map<String, Value>, rusqlite::Error> {
    const TABLES: &[&str] = &[
        "app_config", "modules", "enabled_modules", "categories", "counters",
        "items", "tables", "shifts", "sales", "sale_items", "refunds",
        "refund_items", "table_orders", "tokens", "token_items",
        "delivery_platforms", "orders", "order_items", "order_status_events",
        "employees", "attendance", "salary_payments", "expenses",
    ];

    let mut snapshot = Map::new();
    for table in TABLES {
        let mut statement = conn.prepare(&format!("SELECT * FROM \"{}\"", table))?;
        let columns = statement
            .column_names()
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let rows = statement.query_map([], |row| {
            let mut object = Map::new();
            for (index, column) in columns.iter().enumerate() {
                object.insert(column.clone(), sqlite_value(row.get_ref(index)?));
            }
            Ok(Value::Object(object))
        })?;

        let mut values = Vec::new();
        for row in rows {
            values.push(row?);
        }
        snapshot.insert((*table).to_string(), Value::Array(values));
    }
    Ok(snapshot)
}

fn sqlite_value(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(value) => Value::from(value),
        ValueRef::Real(value) => Value::from(value),
        ValueRef::Text(value) => Value::String(String::from_utf8_lossy(value).into_owned()),
        ValueRef::Blob(value) => Value::String(format!("<{} bytes>", value.len())),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatusInfo {
    pub last_synced_sale_id: i64,
    pub last_synced_at: Option<String>,
    pub server_customer_id: Option<i64>,
    pub total_sales_count: i64,
    pub unsynced_count: i64,
    pub is_license_active: bool,
    pub license_prefix: Option<String>,
    pub server_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub success: bool,
    pub synced_sales: usize,
    pub synced_summaries: usize,
    pub server_customer_id: Option<i64>,
    pub message: String,
}

/// Retrieves the current sync tracking status.
pub fn get_sync_status(conn: &Connection, server_url: &str) -> Result<SyncStatusInfo, rusqlite::Error> {
    let meta: Option<(i64, Option<String>, Option<i64>)> = conn
        .query_row(
            "SELECT last_synced_sale_id, last_synced_at, server_customer_id
               FROM pos_sync_meta WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;

    let (last_synced_sale_id, last_synced_at, server_customer_id) =
        meta.unwrap_or((0, None, None));

    let total_sales_count: i64 = conn.query_row("SELECT COUNT(*) FROM sales", [], |r| r.get(0))?;
    let unsynced_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sales WHERE id > ?1",
        params![last_synced_sale_id],
        |r| r.get(0),
    )?;

    let license_key: Option<String> = conn
        .query_row("SELECT license_key FROM license_status WHERE id = 1", [], |r| r.get(0))
        .optional()?;

    let (is_license_active, license_prefix) = match license_key {
        Some(k) if !k.trim().is_empty() => {
            let prefix = k.split('-').next().unwrap_or("").to_string();
            (true, Some(prefix))
        }
        _ => (false, None),
    };

    Ok(SyncStatusInfo {
        last_synced_sale_id,
        last_synced_at,
        server_customer_id,
        total_sales_count,
        unsynced_count,
        is_license_active,
        license_prefix,
        server_url: server_url.to_string(),
    })
}

/// Gathers sales since `since_id` up to `limit` to send to the server.
pub fn get_sales_for_sync(
    conn: &Connection,
    since_id: i64,
    limit: usize,
    currency: &str,
) -> Result<Vec<SyncSalePayload>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT s.id, o.order_no, t.name, u.name, s.total_minor, s.discount_minor, s.tax_minor,
                s.payment_method, s.created_at
           FROM sales s
      LEFT JOIN orders o ON o.id = s.order_id
      LEFT JOIN tables t ON t.id = s.table_id
      LEFT JOIN users u ON u.id = s.cashier_id
          WHERE s.id > ?1
          ORDER BY s.id ASC
          LIMIT ?2",
    )?;

    let rows = stmt.query_map(params![since_id, limit as i64], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<i64>>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, i64>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
        ))
    })?;

    let mut payloads = Vec::new();
    let mut items_stmt = conn.prepare(
        "SELECT i.name, si.qty
           FROM sale_items si
           JOIN items i ON i.id = si.item_id
          WHERE si.sale_id = ?1",
    )?;

    for row in rows {
        let (id, order_no, table_name, cashier_name, total_minor, discount_minor, tax_minor, payment_method, created_at) = row?;

        let items_rows = items_stmt.query_map(params![id], |ir| {
            Ok((ir.get::<_, String>(0)?, ir.get::<_, f64>(1)?))
        })?;

        let mut item_parts = Vec::new();
        let mut items_count = 0i64;
        for ir in items_rows {
            let (name, qty) = ir?;
            items_count += 1;
            let qty_str = if qty.fract() == 0.0 {
                format!("{}", qty as i64)
            } else {
                format!("{:.2}", qty)
            };
            item_parts.push(format!("{} x{}", name, qty_str));
        }

        let items_summary = item_parts.join(", ");

        payloads.push(SyncSalePayload {
            local_sale_id: id,
            order_no,
            table_name,
            cashier_name,
            total_minor,
            discount_minor,
            tax_minor,
            payment_method,
            currency: currency.to_string(),
            items_count: items_count.max(1),
            items_summary,
            sale_created_at: created_at,
        });
    }

    Ok(payloads)
}

/// Gathers aggregated daily summaries for the past `days` days.
pub fn get_recent_daily_summaries(
    conn: &Connection,
    days: u32,
    currency: &str,
) -> Result<Vec<SyncDailySummaryPayload>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT DATE(created_at) AS sdate,
                COALESCE(SUM(total_minor), 0),
                COUNT(*),
                COALESCE(SUM(CASE WHEN payment_method = 'cash' THEN total_minor ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN payment_method = 'card' THEN total_minor ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN payment_method NOT IN ('cash', 'card') THEN total_minor ELSE 0 END), 0)
           FROM sales
          WHERE created_at >= datetime('now', 'localtime', ?1)
          GROUP BY DATE(created_at)
          ORDER BY sdate DESC",
    )?;

    let days_offset = format!("-{} days", days);
    let rows = stmt.query_map(params![days_offset], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, i64>(5)?,
        ))
    })?;

    let mut summaries = Vec::new();
    let mut refund_stmt = conn.prepare(
        "SELECT COALESCE(SUM(total_refund_amount_minor), 0)
           FROM refunds
          WHERE DATE(created_at) = ?1",
    )?;

    for row in rows {
        let (sdate, total_sales_minor, txns, cash_minor, card_minor, other_minor) = row?;
        let refunds_minor: i64 = refund_stmt.query_row(params![sdate], |r| r.get(0)).unwrap_or(0);
        let net_sales_minor = total_sales_minor - refunds_minor;

        summaries.push(SyncDailySummaryPayload {
            summary_date: sdate,
            total_sales_minor,
            refunds_minor,
            net_sales_minor,
            transaction_count: txns,
            cash_minor,
            card_minor,
            other_minor,
            currency: currency.to_string(),
        });
    }

    Ok(summaries)
}

/// Records a successful sync update in the local database.
pub fn record_sync_success(
    conn: &Connection,
    new_last_sale_id: i64,
    server_customer_id: Option<i64>,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO pos_sync_meta (id, last_synced_sale_id, last_synced_at, server_customer_id, updated_at)
         VALUES (1, ?1, datetime('now', 'localtime'), ?2, datetime('now', 'localtime'))
         ON CONFLICT (id) DO UPDATE SET
            last_synced_sale_id = MAX(pos_sync_meta.last_synced_sale_id, excluded.last_synced_sale_id),
            last_synced_at = excluded.last_synced_at,
            server_customer_id = COALESCE(excluded.server_customer_id, pos_sync_meta.server_customer_id),
            updated_at = excluded.updated_at",
        params![new_last_sale_id, server_customer_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::test_conn;

    const URL: &str = "https://diwan.codehunts.co.uk/api";

    fn sale_ids(conn: &Connection) -> Vec<i64> {
        let mut stmt = conn.prepare("SELECT id FROM sales ORDER BY id").unwrap();
        stmt.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
    }

    #[test]
    fn get_sync_status_counts_every_sale_as_unsynced_at_first() {
        let conn = test_conn();
        let total = sale_ids(&conn).len() as i64;
        assert!(total > 2, "the paging tests below need a few seeded sales");
        let status = get_sync_status(&conn, URL).unwrap();
        assert_eq!(status.last_synced_sale_id, 0);
        assert_eq!(status.server_customer_id, None);
        assert_eq!(status.total_sales_count, total);
        assert_eq!(status.unsynced_count, total);
    }

    #[test]
    fn get_sales_for_sync_loads_items_and_pages_by_since_id_and_limit() {
        let conn = test_conn();
        let ids = sale_ids(&conn);
        let all = get_sales_for_sync(&conn, 0, 1000, "PKR").unwrap();
        assert_eq!(all.iter().map(|s| s.local_sale_id).collect::<Vec<_>>(), ids);
        assert!(!all[0].items_summary.is_empty());

        let page = get_sales_for_sync(&conn, ids[0], 2, "PKR").unwrap();
        assert_eq!(page.iter().map(|s| s.local_sale_id).collect::<Vec<_>>(), ids[1..3]);
        assert!(get_sales_for_sync(&conn, *ids.last().unwrap(), 50, "PKR").unwrap().is_empty());
    }

    #[test]
    fn record_sync_success_never_lowers_the_mark_or_forgets_the_customer() {
        let conn = test_conn();
        record_sync_success(&conn, 10, Some(42)).unwrap();
        record_sync_success(&conn, 4, None).unwrap();
        let status = get_sync_status(&conn, URL).unwrap();
        assert_eq!(status.last_synced_sale_id, 10);
        assert_eq!(status.server_customer_id, Some(42));
        assert!(status.last_synced_at.is_some());

        record_sync_success(&conn, 11, Some(43)).unwrap();
        let status = get_sync_status(&conn, URL).unwrap();
        assert_eq!(status.last_synced_sale_id, 11);
        assert_eq!(status.server_customer_id, Some(43));
    }
}
