//! A shared row on the wire, and back.
//!
//! On the wire a row is a JSON object of its shared columns, where every
//! foreign key is the referenced row's gid instead of this counter's local
//! id. Two references are not declared foreign keys and are translated by
//! hand: `stock_movements.reference_id` (a goods receipt or a stock take,
//! depending on `reason`) and the item ids inside `table_orders.cart_json`.

use rusqlite::types::{Value as Sql, ValueRef};
use rusqlite::{params, params_from_iter, Connection, ErrorCode, OptionalExtension};
use serde_json::{Map, Value};

use super::local::shared_columns;
use super::pins;
use super::spec::{self, TableSpec, APP_CONFIG, CONFIG_GID, ENABLED_MODULES};

#[derive(Debug, thiserror::Error)]
pub enum ApplyError {
    /// The row points at a record this counter does not have yet.
    #[error("waiting for {0}")]
    Unresolved(String),
    /// A local rule refused it (a duplicate name or barcode, a CHECK).
    #[error("{0}")]
    Constraint(String),
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Applied {
    Inserted(i64),
    Updated(i64),
    Deleted,
    /// This counter has its own unsent change to the row: that one goes to
    /// the server next and wins there, so the download is not applied.
    KeptLocal,
    /// Nothing to do (already gone, or a module this build doesn't have).
    Ignored,
    /// Could not be deleted because local records use it; archived instead.
    Archived,
}

fn foreign_keys(conn: &Connection, table: &str) -> Result<Vec<(String, String)>, rusqlite::Error> {
    let mut stmt = conn.prepare(&format!("SELECT \"from\", \"table\" FROM pragma_foreign_key_list('{table}')"))?;
    let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    rows.collect()
}

fn gid_of(conn: &Connection, table: &str, id: i64) -> Result<Option<String>, rusqlite::Error> {
    let pk = spec::find(table).map_or("id", |t| t.pk);
    Ok(conn
        .query_row(&format!("SELECT gid FROM {table} WHERE {pk} = ?1"), params![id], |row| row.get::<_, Option<String>>(0))
        .optional()?
        .flatten())
}

pub fn local_id(conn: &Connection, table: &str, gid: &str) -> Result<Option<i64>, rusqlite::Error> {
    if table == APP_CONFIG {
        return Ok(Some(1));
    }
    let pk = spec::find(table).map_or("id", |t| t.pk);
    conn.query_row(&format!("SELECT {pk} FROM {table} WHERE gid = ?1"), params![gid], |row| row.get(0)).optional()
}

fn to_json(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => Value::from(i),
        ValueRef::Real(f) => serde_json::Number::from_f64(f).map_or(Value::Null, Value::Number),
        ValueRef::Text(t) => Value::String(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(_) => Value::Null, // no shared column is a blob
    }
}

fn to_sql(value: &Value) -> Sql {
    match value {
        Value::Null => Sql::Null,
        Value::Bool(b) => Sql::Integer(*b as i64),
        Value::Number(n) => match n.as_i64() {
            Some(i) => Sql::Integer(i),
            None => Sql::Real(n.as_f64().unwrap_or(0.0)),
        },
        Value::String(s) => Sql::Text(s.clone()),
        other => Sql::Text(other.to_string()),
    }
}

/// `stock_movements.reference_id` points at a different table per reason.
fn reference_table(reason: Option<&str>) -> Option<&'static str> {
    match reason {
        Some("grn") => Some("goods_receipts"),
        Some("stock_take") => Some("stock_takes"),
        _ => None,
    }
}

/// A shared row on the wire: its gid and its shared columns.
pub type WireRow = (String, Map<String, Value>);

/// The row as it is sent, or None when it no longer exists.
pub fn read_row(conn: &Connection, table: &TableSpec, row_id: i64) -> Result<Option<WireRow>, rusqlite::Error> {
    let columns = shared_columns(conn, table)?;
    let fks = foreign_keys(conn, table.name)?;
    let select = if columns.is_empty() { String::new() } else { format!(", {}", columns.join(", ")) };
    let row = conn
        .query_row(
            &format!("SELECT gid{select} FROM {t} WHERE {pk} = ?1", t = table.name, pk = table.pk),
            params![row_id],
            |row| {
                let gid: Option<String> = row.get(0)?;
                let mut values = Vec::with_capacity(columns.len());
                for i in 0..columns.len() {
                    values.push(to_json(row.get_ref(i + 1)?));
                }
                Ok((gid, values))
            },
        )
        .optional()?;
    let Some((Some(gid), values)) = row else {
        return Ok(None);
    };

    let mut data = Map::new();
    for (column, value) in columns.iter().zip(values) {
        let value = match (fks.iter().find(|(c, _)| c == column), value) {
            (Some((_, parent)), Value::Number(n)) if parent != "modules" => {
                n.as_i64().and_then(|id| gid_of(conn, parent, id).transpose()).transpose()?.map_or(Value::Null, Value::String)
            }
            (_, value) => value,
        };
        data.insert(column.clone(), value);
    }

    if table.name == "stock_movements" {
        let target = reference_table(data.get("reason").and_then(Value::as_str));
        if let (Some(target), Some(id)) = (target, data.get("reference_id").and_then(Value::as_i64)) {
            data.insert("reference_id".into(), gid_of(conn, target, id)?.map_or(Value::Null, Value::String));
        }
    }
    if table.name == "table_orders" {
        if let Some(Value::String(json)) = data.get("cart_json").cloned() {
            data.insert("cart_json".into(), Value::String(translate_cart(&json, |id| {
                id.as_i64().map(|id| gid_of(conn, "items", id)).transpose().map(|g| g.flatten().map(Value::String))
            })?));
        }
    }
    pins::seal_row(conn, table.name, &mut data)?;
    Ok(Some((gid, data)))
}

/// Rewrites every line's `itemId` in a parked cart with `map`; a cart that
/// isn't the expected shape is passed through untouched.
fn translate_cart(
    json: &str,
    mut map: impl FnMut(&Value) -> Result<Option<Value>, rusqlite::Error>,
) -> Result<String, rusqlite::Error> {
    let Ok(mut cart) = serde_json::from_str::<Value>(json) else {
        return Ok(json.to_string());
    };
    if let Some(lines) = cart.get_mut("items").and_then(Value::as_array_mut) {
        for line in lines {
            if let Some(item) = line.get_mut("itemId") {
                *item = map(item)?.unwrap_or(Value::Null);
            }
        }
    }
    Ok(cart.to_string())
}

fn has_pending_change(conn: &Connection, table: &str, row_id: i64) -> Result<bool, rusqlite::Error> {
    conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sync_outbox WHERE kind = 'row' AND entity = ?1 AND row_id = ?2)",
        params![table, row_id],
        |row| row.get(0),
    )
}

fn constraint(e: rusqlite::Error) -> ApplyError {
    match &e {
        rusqlite::Error::SqliteFailure(f, message) if f.code == ErrorCode::ConstraintViolation => {
            ApplyError::Constraint(message.clone().unwrap_or_else(|| "constraint failed".into()))
        }
        _ => ApplyError::Sql(e),
    }
}

/// Inserts or updates the local copy of a downloaded row.
pub fn apply_row(conn: &Connection, table: &TableSpec, gid: &str, data: &Map<String, Value>) -> Result<Applied, ApplyError> {
    let existing = if table.name == ENABLED_MODULES {
        let key = gid.strip_prefix("module:").unwrap_or_default();
        let module: Option<i64> =
            conn.query_row("SELECT id FROM modules WHERE key = ?1", params![key], |row| row.get(0)).optional()?;
        let Some(module) = module else {
            return Ok(Applied::Ignored); // a module this build doesn't know
        };
        Some(module)
    } else {
        local_id(conn, table.name, gid)?
    };
    if let Some(id) = existing {
        if has_pending_change(conn, table.name, id)? {
            return Ok(Applied::KeptLocal);
        }
    }

    // PIN hashes arrive sealed (see `pins`). One sealed with another
    // licence key keeps the local hash, and this counter's own copy is sent
    // back; a new row cannot be created without it.
    let mut opened;
    let mut resend = Vec::new();
    let data = if pins::protected_columns(table.name).is_empty() {
        data
    } else {
        opened = data.clone();
        let unreadable = pins::open_row(conn, table.name, &mut opened)?;
        if !unreadable.is_empty() {
            if existing.is_none() && table.name != APP_CONFIG {
                return Err(ApplyError::Unresolved("a staff PIN protected with an earlier licence key".into()));
            }
            resend = unreadable;
        }
        &opened
    };

    let fks = foreign_keys(conn, table.name)?;
    let mut columns = Vec::new();
    let mut values = Vec::new();
    for column in shared_columns(conn, table)? {
        let Some(value) = data.get(&column) else {
            continue; // a column the sending counter doesn't have: keep ours
        };
        let value = match (fks.iter().find(|(c, _)| *c == column), value) {
            (Some((_, parent)), Value::String(parent_gid)) => match local_id(conn, parent, parent_gid)? {
                Some(id) => Sql::Integer(id),
                None => return Err(ApplyError::Unresolved(format!("{parent} {parent_gid}"))),
            },
            (_, value) => to_sql(value),
        };
        columns.push(column);
        values.push(value);
    }

    if table.name == "stock_movements" {
        let reason = data.get("reason").and_then(Value::as_str);
        if let (Some(target), Some(Value::String(ref_gid))) = (reference_table(reason), data.get("reference_id")) {
            let id = local_id(conn, target, ref_gid)?.ok_or_else(|| ApplyError::Unresolved(format!("{target} {ref_gid}")))?;
            if let Some(i) = columns.iter().position(|c| c == "reference_id") {
                values[i] = Sql::Integer(id);
            }
        }
    }
    if table.name == "table_orders" {
        if let Some(i) = columns.iter().position(|c| c == "cart_json") {
            if let Sql::Text(json) = &values[i] {
                let mut missing = None;
                let translated = translate_cart(json, |item| {
                    let Value::String(item_gid) = item else { return Ok(Some(item.clone())) };
                    let id = local_id(conn, "items", item_gid)?;
                    if id.is_none() {
                        missing = Some(item_gid.clone());
                    }
                    Ok(id.map(Value::from))
                })?;
                if let Some(item_gid) = missing {
                    return Err(ApplyError::Unresolved(format!("items {item_gid}")));
                }
                values[i] = Sql::Text(translated);
            }
        }
    }

    match existing {
        Some(id) => {
            if !columns.is_empty() {
                let set = columns.iter().enumerate().map(|(i, c)| format!("{c} = ?{}", i + 1)).collect::<Vec<_>>().join(", ");
                values.push(Sql::Integer(id));
                conn.execute(
                    &format!("UPDATE {t} SET {set} WHERE {pk} = ?{n}", t = table.name, pk = table.pk, n = values.len()),
                    params_from_iter(values.iter()),
                )
                .map_err(constraint)?;
            }
            if !resend.is_empty() {
                conn.execute(
                    "INSERT INTO sync_outbox (kind, entity, row_id, fields, base)
                     VALUES ('row', ?1, ?2, ?3, (SELECT cursor FROM sync_state WHERE id = 1))",
                    params![table.name, id, format!(",{},", resend.join(","))],
                )?;
            }
            // A row archived locally after a refused delete (see apply_delete)
            // comes back through here once the server has it again.
            Ok(Applied::Updated(id))
        }
        None if table.name == APP_CONFIG => Ok(Applied::Ignored),
        None => {
            columns.push("gid".into());
            values.push(Sql::Text(gid.to_string()));
            let placeholders = (1..=values.len()).map(|i| format!("?{i}")).collect::<Vec<_>>().join(", ");
            conn.execute(
                &format!("INSERT INTO {t} ({}) VALUES ({placeholders})", columns.join(", "), t = table.name),
                params_from_iter(values.iter()),
            )
            .map_err(constraint)?;
            Ok(Applied::Inserted(conn.last_insert_rowid()))
        }
    }
}

/// Removes the local copy of a row deleted elsewhere. A row this counter's
/// own records still use (a product sold here while another counter deleted
/// it) is archived instead and sent back, so the business keeps it.
pub fn apply_delete(conn: &Connection, table: &TableSpec, gid: &str) -> Result<Applied, ApplyError> {
    if table.name == APP_CONFIG || table.name == ENABLED_MODULES || gid == CONFIG_GID {
        return Ok(Applied::Ignored);
    }
    let Some(id) = local_id(conn, table.name, gid)? else {
        return Ok(Applied::Ignored);
    };
    if has_pending_change(conn, table.name, id)? {
        return Ok(Applied::KeptLocal);
    }
    match conn.execute(&format!("DELETE FROM {t} WHERE {pk} = ?1", t = table.name, pk = table.pk), params![id]) {
        Ok(_) => Ok(Applied::Deleted),
        Err(rusqlite::Error::SqliteFailure(f, _)) if f.code == ErrorCode::ConstraintViolation => {
            let archivable: bool = conn.query_row(
                &format!("SELECT COUNT(*) > 0 FROM pragma_table_info('{}') WHERE name = 'is_active'", table.name),
                [],
                |row| row.get(0),
            )?;
            if !archivable {
                return Err(ApplyError::Constraint("still used by records on this counter".into()));
            }
            conn.execute(&format!("UPDATE {t} SET is_active = 0 WHERE {pk} = ?1", t = table.name, pk = table.pk), params![id])?;
            conn.execute("INSERT INTO sync_outbox (kind, entity, row_id) VALUES ('row', ?1, ?2)", params![table.name, id])?;
            Ok(Applied::Archived)
        }
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::test_conn;

    #[test]
    fn foreign_keys_travel_as_gids_and_come_back_as_local_ids() {
        let a = test_conn();
        a.execute("INSERT INTO categories (name) VALUES ('Drinks')", []).unwrap();
        let category = a.last_insert_rowid();
        a.execute("INSERT INTO items (name, price_minor, category_id, stock_qty) VALUES ('Cola', 150, ?1, 9)", [category]).unwrap();
        let item = a.last_insert_rowid();
        let (category_gid, category_row) = read_row(&a, spec::find("categories").unwrap(), category).unwrap().unwrap();
        let (item_gid, item_row) = read_row(&a, spec::find("items").unwrap(), item).unwrap().unwrap();
        assert_eq!(item_row["category_id"], Value::String(category_gid.clone()));
        assert!(!item_row.contains_key("stock_qty"), "stock never travels with the row");
        assert!(!item_row.contains_key("id"));

        let b = test_conn();
        // Make b's local ids differ from a's.
        b.execute("INSERT INTO categories (name) VALUES ('Padding 1'), ('Padding 2')", []).unwrap();
        assert!(matches!(
            apply_row(&b, spec::find("items").unwrap(), &item_gid, &item_row),
            Err(ApplyError::Unresolved(_))
        ));
        let Applied::Inserted(b_category) = apply_row(&b, spec::find("categories").unwrap(), &category_gid, &category_row).unwrap() else {
            panic!()
        };
        let Applied::Inserted(b_item) = apply_row(&b, spec::find("items").unwrap(), &item_gid, &item_row).unwrap() else { panic!() };
        let (name, category_id): (String, i64) =
            b.query_row("SELECT name, category_id FROM items WHERE id = ?1", [b_item], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!((name.as_str(), category_id), ("Cola", b_category));
        assert_ne!(b_category, category, "local ids differ between counters");
    }

    #[test]
    fn parked_cart_item_ids_are_translated() {
        let a = test_conn();
        a.execute("INSERT INTO items (name, price_minor) VALUES ('Tea', 100)", []).unwrap();
        let item = a.last_insert_rowid();
        a.execute("INSERT INTO tables (name, seats) VALUES ('T1', 2)", []).unwrap();
        let table = a.last_insert_rowid();
        let cart = format!(r#"{{"items":[{{"itemId":{item},"qty":2.0}}],"discount_minor":0}}"#);
        a.execute("INSERT INTO table_orders (table_id, status, cart_json) VALUES (?1, 'open', ?2)", params![table, cart]).unwrap();
        let order = a.last_insert_rowid();
        let item_gid: String = a.query_row("SELECT gid FROM items WHERE id = ?1", [item], |r| r.get(0)).unwrap();
        let (_, row) = read_row(&a, spec::find("table_orders").unwrap(), order).unwrap().unwrap();
        assert!(row["cart_json"].as_str().unwrap().contains(&item_gid));
    }

    #[test]
    fn a_duplicate_name_is_a_constraint_error_not_a_crash() {
        let a = test_conn();
        let mut data = Map::new();
        data.insert("name".into(), Value::String("Drinks".into()));
        a.execute("INSERT INTO categories (name) VALUES ('Drinks')", []).unwrap();
        assert!(matches!(
            apply_row(&a, spec::find("categories").unwrap(), "ffff:9", &data),
            Err(ApplyError::Constraint(_))
        ));
    }
}
