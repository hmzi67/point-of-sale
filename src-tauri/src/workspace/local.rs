//! The counter's side of the business workspace inside SQLite.
//!
//! - **Global ids.** Every shared table gets a `gid` column, filled by an
//!   AFTER INSERT trigger as `<install id>:<local id>`, so no INSERT in the
//!   app had to learn about it.
//! - **Change capture.** While connected, triggers queue every local insert,
//!   update and delete of a shared table in `sync_outbox`; the push re-reads
//!   the row, so ten edits of one product are sent once. They are quiet while
//!   `sync_state.applying` is set, so a downloaded change is never echoed.
//! - **Stock is a sum, never a value.** `items.stock_qty` is not sent with the
//!   row. A trigger turns every change of it into a signed delta (tagged
//!   with the sale's reservation key while a sale is being recorded), and
//!   the local figure is kept as *central balance + this counter's unsent
//!   deltas* (`recompute_stock`). Two counters selling at once therefore add
//!   up instead of overwriting each other.
//!
//! Triggers are dropped and re-created on every launch, after the schema's
//! own table rebuilds (which drop a table's triggers with it).

use argon2::password_hash::rand_core::{OsRng, RngCore};
use rusqlite::{params, Connection, OptionalExtension};

use super::spec::{TableSpec, APP_CONFIG, CONFIG_COLUMNS, CONFIG_GID, ENABLED_MODULES, ITEMS, TABLES};

/// Trigger condition: connected, and not applying downloaded changes.
const CAPTURING: &str = "(SELECT mode = 'connected' AND applying = 0 FROM sync_state WHERE id = 1)";

/// 16 lowercase hex characters from the OS random source.
pub fn random_hex16() -> String {
    let mut bytes = [0u8; 8];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// False when the state row does not exist yet (a database being created).
pub fn is_connected(conn: &Connection) -> Result<bool, rusqlite::Error> {
    Ok(conn
        .query_row("SELECT mode = 'connected' FROM sync_state WHERE id = 1", [], |row| row.get::<_, bool>(0))
        .optional()?
        .unwrap_or(false))
}

pub fn install_id(conn: &Connection) -> Result<String, rusqlite::Error> {
    conn.query_row("SELECT install_id FROM sync_state WHERE id = 1", [], |row| row.get(0))
}

pub fn install(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT OR IGNORE INTO sync_state (id, install_id) VALUES (1, ?1)",
        params![random_hex16()],
    )?;
    // A crash mid-apply rolls the transaction back, so this is belt and braces.
    conn.execute("UPDATE sync_state SET applying = 0, stock_tag = NULL WHERE id = 1", [])?;

    for table in TABLES {
        ensure_gid_column(conn, table.name)?;
    }
    assign_missing_gids(conn)?;

    // Order numbers restart per counter and day when there is no shift; two
    // counters may both have today's #12, so the uniqueness is per counter.
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_orders_day_no;
         CREATE UNIQUE INDEX idx_orders_day_no ON orders (substr(gid, 1, 16), date(created_at), order_no) WHERE shift_id IS NULL;",
    )?;

    for table in TABLES {
        install_triggers(conn, table)?;
    }
    queue_upgrades(conn)
}

/// Sends, once, what a counter connected by an older build never sent: its
/// photo and logo file names (they stayed on each device) and its staff PIN
/// hashes sealed (they went in the clear). Each as a change of just that
/// field, so nothing else of the record is overwritten with a stale copy.
fn queue_upgrades(conn: &Connection) -> Result<(), rusqlite::Error> {
    if !is_connected(conn)? {
        return Ok(());
    }
    let (media, pins): (bool, bool) =
        conn.query_row("SELECT media_queued, pins_sealed FROM sync_state WHERE id = 1", [], |row| Ok((row.get(0)?, row.get(1)?)))?;
    let queue = |table: &str, field: &str, condition: &str| {
        conn.execute(
            &format!(
                "INSERT INTO sync_outbox (kind, entity, row_id, fields, base)
                   SELECT 'row', '{table}', id, ',{field},', (SELECT cursor FROM sync_state WHERE id = 1)
                     FROM {table} WHERE {condition}"
            ),
            [],
        )
    };
    if !media {
        queue(ITEMS, "image_path", "image_path IS NOT NULL")?;
        queue(APP_CONFIG, "logo_path", "logo_path IS NOT NULL")?;
        conn.execute("UPDATE sync_state SET media_queued = 1 WHERE id = 1", [])?;
    }
    if !pins {
        queue("users", "pin_hash", "1")?;
        queue(APP_CONFIG, "sensitive_area_pin_hash", "sensitive_area_pin_hash IS NOT NULL")?;
        conn.execute("UPDATE sync_state SET pins_sealed = 1 WHERE id = 1", [])?;
    }
    Ok(())
}

fn ensure_gid_column(conn: &Connection, table: &str) -> Result<(), rusqlite::Error> {
    let has: bool = conn.query_row(
        &format!("SELECT COUNT(*) > 0 FROM pragma_table_info('{table}') WHERE name = 'gid'"),
        [],
        |row| row.get(0),
    )?;
    if !has {
        crate::startup_log::log(&format!("db: adding column {table}.gid"));
        conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN gid TEXT"))?;
    }
    conn.execute_batch(&format!("CREATE UNIQUE INDEX IF NOT EXISTS idx_{table}_gid ON {table} (gid)"))
}

/// Rows from before this feature (or written while the triggers were being
/// re-created) get their gid here. Module rows and the config row have
/// fixed gids; every other row gets this installation's prefix.
pub fn assign_missing_gids(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute(&format!("UPDATE {APP_CONFIG} SET gid = ?1 WHERE gid IS NULL"), params![CONFIG_GID])?;
    conn.execute(
        &format!(
            "UPDATE {ENABLED_MODULES} SET gid = 'module:' || (SELECT key FROM modules WHERE modules.id = {ENABLED_MODULES}.module_id)
              WHERE gid IS NULL"
        ),
        [],
    )?;
    for table in TABLES.iter().filter(|t| t.name != APP_CONFIG && t.name != ENABLED_MODULES) {
        conn.execute(
            &format!(
                "UPDATE {t} SET gid = (SELECT install_id FROM sync_state WHERE id = 1) || ':' || {pk} WHERE gid IS NULL",
                t = table.name,
                pk = table.pk
            ),
            [],
        )?;
    }
    Ok(())
}

/// The columns whose change is worth sending: everything but the key, the
/// gid and the device's own columns.
pub fn shared_columns(conn: &Connection, table: &TableSpec) -> Result<Vec<String>, rusqlite::Error> {
    let mut stmt = conn.prepare(&format!("SELECT name FROM pragma_table_info('{}')", table.name))?;
    let columns = stmt.query_map([], |row| row.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
    Ok(columns
        .into_iter()
        .filter(|c| {
            if table.name == APP_CONFIG {
                return CONFIG_COLUMNS.contains(&c.as_str());
            }
            c != table.pk && c != "gid" && !table.local_columns.contains(&c.as_str())
        })
        .collect())
}

fn install_triggers(conn: &Connection, table: &TableSpec) -> Result<(), rusqlite::Error> {
    let t = table.name;
    let pk = table.pk;
    let mut sql = format!(
        "DROP TRIGGER IF EXISTS ws_{t}_gid; DROP TRIGGER IF EXISTS ws_{t}_ins;
         DROP TRIGGER IF EXISTS ws_{t}_upd; DROP TRIGGER IF EXISTS ws_{t}_del;"
    );

    let gid_expr = match t {
        APP_CONFIG => format!("'{CONFIG_GID}'"),
        ENABLED_MODULES => format!("'module:' || (SELECT key FROM modules WHERE id = NEW.{pk})"),
        _ => format!("(SELECT install_id FROM sync_state WHERE id = 1) || ':' || NEW.{pk}"),
    };
    sql.push_str(&format!(
        "CREATE TRIGGER ws_{t}_gid AFTER INSERT ON {t} WHEN NEW.gid IS NULL
         BEGIN UPDATE {t} SET gid = {gid_expr} WHERE {pk} = NEW.{pk}; END;"
    ));

    let columns = shared_columns(conn, table)?;
    if t != APP_CONFIG {
        sql.push_str(&format!(
            "CREATE TRIGGER ws_{t}_ins AFTER INSERT ON {t} WHEN {CAPTURING}
             BEGIN INSERT INTO sync_outbox (kind, entity, row_id) VALUES ('row', '{t}', NEW.{pk}); END;
             CREATE TRIGGER ws_{t}_del AFTER DELETE ON {t} WHEN {CAPTURING}
             BEGIN INSERT INTO sync_outbox (kind, entity, row_id, gid, op) VALUES ('row', '{t}', OLD.{pk}, OLD.gid, 'delete'); END;"
        ));
    }
    if !columns.is_empty() {
        // Only an update that changed something is queued, with the names of
        // what it changed and this counter's cursor at that moment (see
        // `engine::build_push` and the server's field merge).
        let changed = columns.iter().map(|c| format!("NEW.{c} IS NOT OLD.{c}")).collect::<Vec<_>>().join(" OR ");
        let names = columns.iter().map(|c| format!("(CASE WHEN NEW.{c} IS NOT OLD.{c} THEN '{c},' ELSE '' END)")).collect::<Vec<_>>().join(" || ");
        sql.push_str(&format!(
            "CREATE TRIGGER ws_{t}_upd AFTER UPDATE OF {cols} ON {t} WHEN {CAPTURING} AND ({changed})
             BEGIN INSERT INTO sync_outbox (kind, entity, row_id, fields, base)
                   VALUES ('row', '{t}', NEW.{pk}, ',' || {names}, (SELECT cursor FROM sync_state WHERE id = 1)); END;",
            cols = columns.join(", ")
        ));
    }

    if t == ITEMS {
        sql.push_str(&format!(
            "DROP TRIGGER IF EXISTS ws_items_stock; DROP TRIGGER IF EXISTS ws_items_stock_ins;
             CREATE TRIGGER ws_items_stock AFTER UPDATE OF stock_qty ON items
               WHEN NEW.stock_qty <> OLD.stock_qty AND {CAPTURING}
             BEGIN INSERT INTO sync_outbox (kind, entity, row_id, qty, tag)
                   VALUES ('stock', 'items', NEW.id, round(NEW.stock_qty - OLD.stock_qty, 4),
                           (SELECT stock_tag FROM sync_state WHERE id = 1)); END;
             CREATE TRIGGER ws_items_stock_ins AFTER INSERT ON items
               WHEN NEW.stock_qty <> 0 AND {CAPTURING}
             BEGIN INSERT INTO sync_outbox (kind, entity, row_id, qty) VALUES ('stock', 'items', NEW.id, round(NEW.stock_qty, 4)); END;"
        ));
    }
    conn.execute_batch(&sql)
}

/// The counter of the business that created a record (from its gid), as
/// `("C2", "Front counter")` for receipts and kitchen tokens. None while the
/// business has a single counter, or when the creator is not known (a
/// record from before this counter joined, or from a reinstalled one).
pub fn till_of(conn: &Connection, gid: Option<&str>) -> Result<Option<(String, String)>, rusqlite::Error> {
    let Some(prefix) = gid.and_then(|g| g.split_once(':')).map(|(p, _)| p) else {
        return Ok(None);
    };
    conn.query_row(
        "SELECT 'C' || seq, name FROM sync_terminals
          WHERE install_id = ?1 AND seq IS NOT NULL AND (SELECT COUNT(*) FROM sync_terminals) > 1",
        params![prefix],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
}

/// This counter's own label (`C2`), for what it prints before the record
/// exists.
pub fn this_till(conn: &Connection) -> Result<Option<(String, String)>, rusqlite::Error> {
    let install: Option<String> = conn.query_row("SELECT install_id FROM sync_state WHERE id = 1", [], |r| r.get(0)).optional()?;
    till_of(conn, install.map(|i| format!("{i}:0")).as_deref())
}

/// The number a receipt shows: the creating counter's own id prefixed with
/// its label (`C2-12`), the same on every counter; just the id otherwise.
pub fn receipt_number(id: i64, gid: Option<&str>, till: Option<&(String, String)>) -> String {
    match (till, gid.and_then(|g| g.split_once(':'))) {
        (Some((label, _)), Some((_, origin_id))) => format!("{label}-{origin_id}"),
        _ => id.to_string(),
    }
}

/// Marks the start/end of applying downloaded changes (see module doc).
pub fn set_applying(conn: &Connection, applying: bool) -> Result<(), rusqlite::Error> {
    conn.execute("UPDATE sync_state SET applying = ?1 WHERE id = 1", params![applying as i64])?;
    Ok(())
}

/// Tags the stock changes written until it is cleared (a sale's reservation key).
pub fn set_stock_tag(conn: &Connection, tag: Option<&str>) -> Result<(), rusqlite::Error> {
    conn.execute("UPDATE sync_state SET stock_tag = ?1 WHERE id = 1", params![tag])?;
    Ok(())
}

/// This counter's stock changes for `item_id` that the central balance does
/// not include yet: everything queued, except a reserved sale's own changes
/// (the reservation already took them centrally).
pub fn pending_stock(conn: &Connection, item_id: i64) -> Result<f64, rusqlite::Error> {
    conn.query_row(
        "SELECT COALESCE(SUM(o.qty), 0) FROM sync_outbox o
          WHERE o.kind = 'stock' AND o.row_id = ?1
            AND NOT EXISTS (SELECT 1 FROM sync_reservation_lines l
                              JOIN sync_reservations r ON r.key = l.key AND r.status IN ('confirmed', 'used')
                              JOIN items i ON i.id = o.row_id
                             WHERE l.key = o.tag AND l.item_gid = i.gid)",
        params![item_id],
        |row| row.get(0),
    )
}

/// `stock_qty = central balance + pending`. Call with `applying` set, or the
/// write would be captured as a new local change.
pub fn recompute_stock(conn: &Connection, item_id: i64) -> Result<(), rusqlite::Error> {
    let central: Option<f64> = conn
        .query_row(
            "SELECT c.qty FROM sync_central_stock c JOIN items i ON i.gid = c.item_gid WHERE i.id = ?1",
            params![item_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(central) = central else {
        return Ok(()); // the server has no balance yet: the local one stands
    };
    let qty = ((central + pending_stock(conn, item_id)?) * 10_000.0).round() / 10_000.0;
    conn.execute("UPDATE items SET stock_qty = ?1 WHERE id = ?2 AND stock_qty <> ?1", params![qty, item_id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::test_conn;

    fn connect(conn: &Connection) {
        conn.execute("UPDATE sync_state SET mode = 'connected' WHERE id = 1", []).unwrap();
    }

    fn outbox(conn: &Connection) -> Vec<(String, String, Option<f64>)> {
        let mut stmt = conn.prepare("SELECT kind, entity, qty FROM sync_outbox ORDER BY id").unwrap();
        stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).unwrap().map(Result::unwrap).collect()
    }

    #[test]
    fn every_row_gets_a_gid_with_this_installations_prefix() {
        let conn = test_conn();
        let prefix = install_id(&conn).unwrap();
        conn.execute("INSERT INTO categories (name) VALUES ('Drinks')", []).unwrap();
        let gid: String = conn.query_row("SELECT gid FROM categories WHERE name = 'Drinks'", [], |r| r.get(0)).unwrap();
        let id: i64 = conn.query_row("SELECT id FROM categories WHERE name = 'Drinks'", [], |r| r.get(0)).unwrap();
        assert_eq!(gid, format!("{prefix}:{id}"));
        let config: String = conn.query_row("SELECT gid FROM app_config", [], |r| r.get(0)).unwrap();
        assert_eq!(config, "config");
        let module: String = conn
            .query_row("SELECT e.gid FROM enabled_modules e JOIN modules m ON m.id = e.module_id WHERE m.key = 'billing'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(module, "module:billing");
        let missing: i64 = conn.query_row("SELECT COUNT(*) FROM users WHERE gid IS NULL", [], |r| r.get(0)).unwrap();
        assert_eq!(missing, 0, "seeded rows got gids too");
    }

    #[test]
    fn nothing_is_queued_while_standalone() {
        let conn = test_conn();
        conn.execute("INSERT INTO categories (name) VALUES ('Standalone only')", []).unwrap();
        assert!(outbox(&conn).is_empty());
    }

    #[test]
    fn connected_writes_are_queued_and_stock_changes_become_deltas() {
        let conn = test_conn();
        connect(&conn);
        conn.execute("INSERT INTO items (name, price_minor, stock_qty) VALUES ('Tea', 100, 10)", []).unwrap();
        let id = conn.last_insert_rowid();
        conn.execute("UPDATE items SET stock_qty = stock_qty - 3 WHERE id = ?1", [id]).unwrap();
        conn.execute("UPDATE items SET price_minor = 120 WHERE id = ?1", [id]).unwrap();
        conn.execute("UPDATE app_config SET theme = 'light'", []).unwrap(); // per device: not queued
        conn.execute("UPDATE app_config SET tax_percent = 7.5", []).unwrap();
        // Two triggers fire on the insert, in an order SQLite doesn't promise.
        let mut queued = outbox(&conn);
        queued.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
        assert_eq!(
            queued,
            vec![
                ("row".into(), "app_config".into(), None),
                ("row".into(), "items".into(), None),
                ("row".into(), "items".into(), None),
                ("stock".into(), "items".into(), Some(-3.0)),
                ("stock".into(), "items".into(), Some(10.0)),
            ]
        );
    }

    #[test]
    fn applying_downloaded_changes_is_not_captured() {
        let conn = test_conn();
        connect(&conn);
        set_applying(&conn, true).unwrap();
        conn.execute("INSERT INTO items (name, price_minor, stock_qty, gid) VALUES ('Remote', 100, 4, 'abcd:1')", []).unwrap();
        set_applying(&conn, false).unwrap();
        assert!(outbox(&conn).is_empty());
        let gid: String = conn.query_row("SELECT gid FROM items WHERE name = 'Remote'", [], |r| r.get(0)).unwrap();
        assert_eq!(gid, "abcd:1", "a downloaded row keeps its own gid");
    }

    #[test]
    fn receipts_name_the_counter_once_the_business_has_two() {
        let conn = test_conn();
        let insert = "INSERT INTO sync_terminals (code, name, install_id, seq) VALUES (?1, ?2, ?3, ?4)";
        conn.execute(insert, params!["AAAA", "Front", "aaaa000000000001", 1]).unwrap();
        assert_eq!(till_of(&conn, Some("aaaa000000000001:12")).unwrap(), None, "one counter: receipts unchanged");
        assert_eq!(receipt_number(12, Some("aaaa000000000001:12"), None), "12");

        conn.execute(insert, params!["BBBB", "Takeaway", "bbbb000000000002", 2]).unwrap();
        let till = till_of(&conn, Some("bbbb000000000002:7")).unwrap();
        assert_eq!(till, Some(("C2".to_string(), "Takeaway".to_string())));
        // Counter 1 shows the other counter's sale 7 (local id 31 here) as C2-7.
        assert_eq!(receipt_number(31, Some("bbbb000000000002:7"), till.as_ref()), "C2-7");
        assert_eq!(till_of(&conn, Some("ffff000000000009:1")).unwrap(), None, "unknown creator");
        assert_eq!(till_of(&conn, None).unwrap(), None);
    }

    #[test]
    fn local_stock_is_the_central_balance_plus_unsent_changes() {
        let conn = test_conn();
        connect(&conn);
        conn.execute("INSERT INTO items (name, price_minor, stock_qty) VALUES ('Rice', 100, 0)", []).unwrap();
        let id = conn.last_insert_rowid();
        let gid: String = conn.query_row("SELECT gid FROM items WHERE id = ?1", [id], |r| r.get(0)).unwrap();
        conn.execute("UPDATE items SET stock_qty = stock_qty - 2 WHERE id = ?1", [id]).unwrap(); // unsent sale
        conn.execute("INSERT INTO sync_central_stock (item_gid, qty) VALUES (?1, 92)", [&gid]).unwrap();
        set_applying(&conn, true).unwrap();
        recompute_stock(&conn, id).unwrap();
        set_applying(&conn, false).unwrap();
        let qty: f64 = conn.query_row("SELECT stock_qty FROM items WHERE id = ?1", [id], |r| r.get(0)).unwrap();
        assert_eq!(qty, 90.0);
    }
}
