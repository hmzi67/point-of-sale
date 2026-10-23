//! Stock control outside of selling: manual adjustments (stock added,
//! damage written off, corrections), stock takes, generated barcodes and
//! weight-scale barcode lookup.
//!
//! Every stock change here writes a `stock_movements` row next to the
//! `items.stock_qty` update, in the same transaction, so the movement
//! history always explains the number on the shelf. Goods received go
//! through `db::purchasing::receive_goods`, which writes the same rows.

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};

use crate::db::config::AppConfig;
use crate::db::items::{self, Item};

#[derive(Debug, thiserror::Error)]
pub enum StockError {
    #[error("Item {0} not found")]
    ItemNotFound(i64),
    #[error("{0}")]
    Invalid(String),
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdjustmentInput {
    pub item_id: i64,
    /// `add` (qty > 0 in), `damage` (qty > 0 out), `correction` (qty is the
    /// new on-hand count).
    pub kind: String,
    pub qty: f64,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StockMovement {
    pub id: i64,
    pub item_id: i64,
    pub item_name: String,
    pub unit: Option<String>,
    pub qty_change: f64,
    pub reason: String,
    pub reference_id: Option<i64>,
    pub note: Option<String>,
    pub user_name: Option<String>,
    pub created_at: String,
    /// `qty_change × items.cost_minor` (current cost), rounded.
    pub value_minor: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CountLine {
    pub item_id: i64,
    pub counted_qty: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StockTakeLine {
    pub item_id: i64,
    pub item_name: String,
    pub unit: Option<String>,
    pub expected_qty: f64,
    pub counted_qty: f64,
    pub variance_qty: f64,
    pub variance_value_minor: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StockTake {
    pub id: i64,
    pub note: Option<String>,
    pub created_by_name: Option<String>,
    pub created_at: String,
    pub items_counted: i64,
    pub items_with_variance: i64,
    pub variance_value_minor: i64,
    /// Empty in list views; filled by `get_stock_take`.
    pub lines: Vec<StockTakeLine>,
}

/// A weight-scale barcode resolved to an item and the quantity it encodes.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScaleMatch {
    pub item: Item,
    pub qty: f64,
}

fn round_qty(qty: f64) -> f64 {
    (qty * 1000.0).round() / 1000.0
}

fn item_stock(tx: &Connection, item_id: i64) -> Result<(f64, i64), StockError> {
    tx.query_row(
        "SELECT stock_qty, cost_minor FROM items WHERE id = ?1",
        params![item_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()?
    .ok_or(StockError::ItemNotFound(item_id))
}

/// Writes one movement and applies it to `items.stock_qty`.
pub(crate) fn apply_movement(
    tx: &Connection,
    item_id: i64,
    qty_change: f64,
    reason: &str,
    reference_id: Option<i64>,
    note: Option<&str>,
    user_id: Option<i64>,
) -> Result<(), rusqlite::Error> {
    tx.execute(
        "UPDATE items SET stock_qty = stock_qty + ?1, updated_at = datetime('now', 'localtime') WHERE id = ?2",
        params![qty_change, item_id],
    )?;
    tx.execute(
        "INSERT INTO stock_movements (item_id, qty_change, reason, reference_id, note, user_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![item_id, qty_change, reason, reference_id, note, user_id],
    )?;
    Ok(())
}

/// Adds stock, writes off damage, or corrects the on-hand count. Stock
/// never goes below zero.
pub fn adjust(
    tx: &Transaction,
    input: AdjustmentInput,
    user_id: Option<i64>,
) -> Result<Item, StockError> {
    if !input.qty.is_finite() || input.qty < 0.0 {
        return Err(StockError::Invalid("Quantity must be zero or more".into()));
    }
    let (current, _) = item_stock(tx, input.item_id)?;
    let change = match input.kind.as_str() {
        "add" | "damage" if input.qty == 0.0 => {
            return Err(StockError::Invalid(
                "Quantity must be greater than zero".into(),
            ))
        }
        "add" => input.qty,
        "damage" => {
            if input.qty > current {
                return Err(StockError::Invalid(format!(
                    "Only {current} in stock to write off"
                )));
            }
            -input.qty
        }
        "correction" => input.qty - current,
        other => return Err(StockError::Invalid(format!("Unknown adjustment: {other}"))),
    };
    let change = round_qty(change);
    if change != 0.0 {
        let note = input
            .note
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        apply_movement(tx, input.item_id, change, &input.kind, None, note, user_id)?;
    }
    items::get_item(tx, input.item_id).map_err(|_| StockError::ItemNotFound(input.item_id))
}

/// Movements newest first, optionally for one item and/or a date range.
pub fn list_movements(
    conn: &Connection,
    item_id: Option<i64>,
    start_date: Option<&str>,
    end_date: Option<&str>,
    reason: Option<&str>,
    limit: i64,
) -> Result<Vec<StockMovement>, rusqlite::Error> {
    let start = start_date.map(crate::db::dates::day_start);
    let end = end_date.map(crate::db::dates::day_end);
    let mut stmt = conn.prepare(
        "SELECT m.id, m.item_id, i.name, i.unit, m.qty_change, m.reason, m.reference_id, m.note, u.name,
                m.created_at, i.cost_minor
           FROM stock_movements m
           JOIN items i ON i.id = m.item_id
           LEFT JOIN users u ON u.id = m.user_id
          WHERE (?1 IS NULL OR m.item_id = ?1)
            AND (?2 IS NULL OR m.created_at >= ?2)
            AND (?3 IS NULL OR m.created_at <= ?3)
            AND (?4 IS NULL OR m.reason = ?4)
          ORDER BY m.id DESC
          LIMIT ?5",
    )?;
    let rows = stmt.query_map(
        params![item_id, start, end, reason, limit.clamp(1, 5000)],
        |row| {
            let qty_change: f64 = row.get(4)?;
            let cost: i64 = row.get(10)?;
            Ok(StockMovement {
                id: row.get(0)?,
                item_id: row.get(1)?,
                item_name: row.get(2)?,
                unit: row.get(3)?,
                qty_change,
                reason: row.get(5)?,
                reference_id: row.get(6)?,
                note: row.get(7)?,
                user_name: row.get(8)?,
                created_at: row.get(9)?,
                value_minor: (qty_change * cost as f64).round() as i64,
            })
        },
    )?;
    rows.collect()
}

/// Records a count: each counted item's stock becomes the counted figure,
/// and any difference is logged as a `stock_take` movement.
pub fn submit_stock_take(
    tx: &Transaction,
    note: Option<&str>,
    lines: &[CountLine],
    user_id: Option<i64>,
) -> Result<StockTake, StockError> {
    if lines.is_empty() {
        return Err(StockError::Invalid("Count at least one item".into()));
    }
    let note = note.map(str::trim).filter(|s| !s.is_empty());
    tx.execute(
        "INSERT INTO stock_takes (note, created_by) VALUES (?1, ?2)",
        params![note, user_id],
    )?;
    let take_id = tx.last_insert_rowid();
    let mut seen = Vec::with_capacity(lines.len());
    for line in lines {
        if !line.counted_qty.is_finite() || line.counted_qty < 0.0 {
            return Err(StockError::Invalid(
                "Counted quantities must be zero or more".into(),
            ));
        }
        if seen.contains(&line.item_id) {
            return Err(StockError::Invalid("An item was counted twice".into()));
        }
        seen.push(line.item_id);
        let (expected, cost) = item_stock(tx, line.item_id)?;
        tx.execute(
            "INSERT INTO stock_take_lines (stock_take_id, item_id, expected_qty, counted_qty, unit_cost_minor)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![take_id, line.item_id, expected, line.counted_qty, cost],
        )?;
        let variance = round_qty(line.counted_qty - expected);
        if variance != 0.0 {
            apply_movement(
                tx,
                line.item_id,
                variance,
                "stock_take",
                Some(take_id),
                note,
                user_id,
            )?;
        }
    }
    get_stock_take(tx, take_id)
}

const TAKE_SELECT: &str = "
    SELECT t.id, t.note, u.name, t.created_at,
           (SELECT COUNT(*) FROM stock_take_lines l WHERE l.stock_take_id = t.id),
           (SELECT COUNT(*) FROM stock_take_lines l WHERE l.stock_take_id = t.id
              AND ABS(l.counted_qty - l.expected_qty) > 0.0005),
           (SELECT COALESCE(SUM(ROUND((l.counted_qty - l.expected_qty) * l.unit_cost_minor)), 0)
              FROM stock_take_lines l WHERE l.stock_take_id = t.id)
      FROM stock_takes t
      LEFT JOIN users u ON u.id = t.created_by";

fn take_from_row(row: &rusqlite::Row) -> rusqlite::Result<StockTake> {
    Ok(StockTake {
        id: row.get(0)?,
        note: row.get(1)?,
        created_by_name: row.get(2)?,
        created_at: row.get(3)?,
        items_counted: row.get(4)?,
        items_with_variance: row.get(5)?,
        variance_value_minor: row.get::<_, f64>(6)? as i64,
        lines: Vec::new(),
    })
}

pub fn list_stock_takes(conn: &Connection, limit: i64) -> Result<Vec<StockTake>, rusqlite::Error> {
    let mut stmt = conn.prepare(&format!("{TAKE_SELECT} ORDER BY t.id DESC LIMIT ?1"))?;
    let rows = stmt.query_map(params![limit.clamp(1, 500)], take_from_row)?;
    rows.collect()
}

pub fn get_stock_take(conn: &Connection, id: i64) -> Result<StockTake, StockError> {
    let mut take = conn
        .query_row(
            &format!("{TAKE_SELECT} WHERE t.id = ?1"),
            params![id],
            take_from_row,
        )
        .optional()?
        .ok_or_else(|| StockError::Invalid(format!("Stock take #{id} not found")))?;
    let mut stmt = conn.prepare(
        "SELECT l.item_id, i.name, i.unit, l.expected_qty, l.counted_qty, l.unit_cost_minor
           FROM stock_take_lines l JOIN items i ON i.id = l.item_id
          WHERE l.stock_take_id = ?1
          ORDER BY ABS(l.counted_qty - l.expected_qty) * l.unit_cost_minor DESC, i.name",
    )?;
    take.lines = stmt
        .query_map(params![id], |row| {
            let expected: f64 = row.get(3)?;
            let counted: f64 = row.get(4)?;
            let cost: i64 = row.get(5)?;
            let variance = round_qty(counted - expected);
            Ok(StockTakeLine {
                item_id: row.get(0)?,
                item_name: row.get(1)?,
                unit: row.get(2)?,
                expected_qty: expected,
                counted_qty: counted,
                variance_qty: variance,
                variance_value_minor: (variance * cost as f64).round() as i64,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(take)
}

// ---------------------------------------------------------------------------
// Barcodes
// ---------------------------------------------------------------------------

/// The EAN-13 check digit for the first 12 digits.
pub fn ean13_check_digit(first12: &str) -> u32 {
    let sum: u32 = first12
        .chars()
        .filter_map(|c| c.to_digit(10))
        .enumerate()
        .map(|(i, d)| if i % 2 == 0 { d } else { d * 3 })
        .sum();
    (10 - sum % 10) % 10
}

/// Gives every listed item that has no barcode an in-store EAN-13 — prefix
/// `200` (the GS1 range reserved for in-store use) + the item id, zero
/// padded, + check digit. `None` covers every active item without one.
/// Returns how many were assigned; an id-derived code that is somehow
/// already taken is skipped rather than overwritten.
pub fn generate_barcodes(
    tx: &Transaction,
    item_ids: Option<&[i64]>,
) -> Result<i64, rusqlite::Error> {
    let ids: Vec<i64> = match item_ids {
        Some(ids) => ids.to_vec(),
        None => {
            let mut stmt = tx.prepare(
                "SELECT id FROM items WHERE is_active = 1 AND (barcode IS NULL OR barcode = '')",
            )?;
            let rows = stmt.query_map([], |row| row.get(0))?;
            rows.collect::<Result<_, _>>()?
        }
    };
    let mut assigned = 0;
    for id in ids {
        let has: Option<String> = tx
            .query_row(
                "SELECT barcode FROM items WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        if has.is_some_and(|b| !b.trim().is_empty()) {
            continue;
        }
        let first12 = format!("200{:09}", id);
        let code = format!("{first12}{}", ean13_check_digit(&first12));
        let taken: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM items WHERE barcode = ?1)",
            params![code],
            |row| row.get(0),
        )?;
        if taken {
            continue;
        }
        tx.execute(
            "UPDATE items SET barcode = ?1, updated_at = datetime('now', 'localtime') WHERE id = ?2",
            params![code, id],
        )?;
        assigned += 1;
    }
    Ok(assigned)
}

/// Splits a scanned code using the configured weight-scale layout — prefix,
/// item code, embedded value, EAN-13 check digit. Returns the item code and
/// the embedded value, or `None` when the code isn't one.
pub fn split_scale_barcode(code: &str, config: &AppConfig) -> Option<(String, u64)> {
    let code = code.trim();
    if !config.scale_barcode_enabled
        || code.len() != 13
        || !code.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    let prefix = config.scale_barcode_prefix.as_str();
    let item_digits = usize::try_from(config.scale_item_code_digits).ok()?;
    if !code.starts_with(prefix) || prefix.len() + item_digits >= 12 {
        return None;
    }
    let check = code[12..].parse::<u32>().ok()?;
    if ean13_check_digit(&code[..12]) != check {
        return None;
    }
    let item_code = code[prefix.len()..prefix.len() + item_digits].to_string();
    let value = code[prefix.len() + item_digits..12].parse::<u64>().ok()?;
    Some((item_code, value))
}

/// Resolves a weight-scale barcode to an active item (by short code, compared
/// numerically so "00042" matches "42", or by barcode) and the quantity —
/// a weight directly, or an embedded price divided by the item's price.
pub fn lookup_scale_barcode(
    conn: &Connection,
    code: &str,
    config: &AppConfig,
) -> Result<Option<ScaleMatch>, rusqlite::Error> {
    let Some((item_code, value)) = split_scale_barcode(code, config) else {
        return Ok(None);
    };
    let numeric: i64 = item_code.parse().unwrap_or(-1);
    let id: Option<i64> = conn
        .query_row(
            "SELECT id FROM items
              WHERE is_active = 1
                AND ((short_code GLOB '[0-9]*' AND CAST(short_code AS INTEGER) = ?1) OR barcode = ?2)
              ORDER BY id LIMIT 1",
            params![numeric, item_code],
            |row| row.get(0),
        )
        .optional()?;
    let Some(id) = id else { return Ok(None) };
    let Ok(item) = items::get_item(conn, id) else {
        return Ok(None);
    };
    let scaled = value as f64 / 10f64.powi(config.scale_value_decimals as i32);
    let qty = if config.scale_value_kind == "price" {
        if item.price_minor <= 0 {
            return Ok(None);
        }
        scaled * 100.0 / item.price_minor as f64
    } else {
        scaled
    };
    if qty <= 0.0 {
        return Ok(None);
    }
    Ok(Some(ScaleMatch { item, qty }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::test_conn;
    use crate::db::test_support::{in_tx, item_id, stock};

    #[test]
    fn adjustments_move_stock_and_leave_a_trail() {
        let mut conn = test_conn();
        let cola = item_id(&conn, "Cola 500ml");
        let before = stock(&conn, "Cola 500ml");
        let add = |kind: &str, qty: f64| AdjustmentInput {
            item_id: cola,
            kind: kind.into(),
            qty,
            note: None,
        };

        in_tx(&mut conn, |tx| adjust(tx, add("add", 10.0), None)).unwrap();
        in_tx(&mut conn, |tx| adjust(tx, add("damage", 3.0), None)).unwrap();
        assert_eq!(stock(&conn, "Cola 500ml"), before + 7.0);
        assert!(in_tx(&mut conn, |tx| adjust(tx, add("damage", 10_000.0), None)).is_err());

        in_tx(&mut conn, |tx| adjust(tx, add("correction", 5.0), None)).unwrap();
        assert_eq!(stock(&conn, "Cola 500ml"), 5.0);
        let moves = list_movements(&conn, Some(cola), None, None, None, 10).unwrap();
        assert_eq!(moves.len(), 3);
        assert_eq!(moves[0].qty_change, 5.0 - (before + 7.0));
    }

    #[test]
    fn a_stock_take_sets_counts_and_records_variance() {
        let mut conn = test_conn();
        let cola = item_id(&conn, "Cola 500ml");
        let before = stock(&conn, "Cola 500ml");
        let take = in_tx(&mut conn, |tx| {
            submit_stock_take(
                tx,
                Some("Month end"),
                &[CountLine {
                    item_id: cola,
                    counted_qty: before - 2.0,
                }],
                None,
            )
        })
        .unwrap();
        assert_eq!(take.items_counted, 1);
        assert_eq!(take.items_with_variance, 1);
        assert_eq!(take.lines[0].variance_qty, -2.0);
        assert_eq!(stock(&conn, "Cola 500ml"), before - 2.0);
    }

    #[test]
    fn generated_barcodes_are_valid_ean13_and_never_overwrite() {
        let mut conn = test_conn();
        conn.execute(
            "UPDATE items SET barcode = NULL WHERE name = 'Cola 500ml'",
            [],
        )
        .unwrap();
        let assigned = in_tx(&mut conn, |tx| generate_barcodes(tx, None)).unwrap();
        assert!(assigned >= 1);
        let code: String = conn
            .query_row(
                "SELECT barcode FROM items WHERE name = 'Cola 500ml'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(code.len(), 13);
        assert_eq!(
            ean13_check_digit(&code[..12]),
            code[12..].parse::<u32>().unwrap()
        );
        assert_eq!(
            in_tx(&mut conn, |tx| generate_barcodes(tx, None)).unwrap(),
            0
        );
    }

    #[test]
    fn scale_barcodes_decode_weight_and_price() {
        let conn = test_conn();
        conn.execute(
            "UPDATE items SET short_code = '42' WHERE name = 'Cola 500ml'",
            [],
        )
        .unwrap();
        let mut config = crate::db::config::get(&conn).unwrap();
        config.scale_barcode_enabled = true;
        // 21 + 00042 + 01250 (1.250 kg) + check
        let first12 = "210004201250";
        let code = format!("{first12}{}", ean13_check_digit(first12));
        let hit = lookup_scale_barcode(&conn, &code, &config)
            .unwrap()
            .unwrap();
        assert_eq!(hit.item.name, "Cola 500ml");
        assert!((hit.qty - 1.25).abs() < 1e-9);

        let bad_check = format!("{first12}{}", (ean13_check_digit(first12) + 1) % 10);
        assert!(lookup_scale_barcode(&conn, &bad_check, &config)
            .unwrap()
            .is_none());

        config.scale_value_kind = "price".into();
        config.scale_value_decimals = 0;
        // 21 + 00042 + 00160 (Rs 160 = two colas at 80.00)
        let first12 = "210004200160";
        let code = format!("{first12}{}", ean13_check_digit(first12));
        let hit = lookup_scale_barcode(&conn, &code, &config)
            .unwrap()
            .unwrap();
        assert!((hit.qty - 2.0).abs() < 1e-9);
    }
}
