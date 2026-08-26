//! Restaurant table management: the floor (`tables`) and each table's
//! in-progress order (`table_orders`).
//!
//! A table's order is still a lightweight stand-in — the cart is a JSON
//! snapshot on `table_orders.cart_json` rather than real relational line
//! items (see the module doc comment history in `db/sales.rs`) — but this
//! module now owns the full lifecycle: creating tables, flipping status,
//! starting an order when a table is seated, and clearing a table either via
//! a completed sale (`close_table_order_for_sale`, called from
//! `sales::create_sale`) or manually (`clear_table`, e.g. a cancelled visit).

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};

const TABLE_STATUSES: &[&str] = &["free", "occupied", "reserved"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableSummary {
    pub id: i64,
    pub name: String,
    pub seats: i64,
    pub status: String,
    pub has_parked_order: bool,
}

/// Both tables' post-shift state — the floor view updates both cards from
/// this one response instead of re-fetching the whole table list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShiftTableResult {
    pub from: TableSummary,
    pub to: TableSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParkedCartLine {
    pub item_id: i64,
    pub qty: i64,
}

/// The JSON payload stored in `table_orders.cart_json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ParkedCart {
    items: Vec<ParkedCartLine>,
    discount_minor: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParkedOrder {
    pub items: Vec<ParkedCartLine>,
    pub discount_minor: i64,
}

#[derive(Debug)]
pub enum TableError {
    TableNotFound,
    EmptyCart,
    EmptyName,
    DuplicateName(String),
    InvalidSeats,
    InvalidStatus(String),
    Corrupt(String),
    Sqlite(rusqlite::Error),
    /// Shift-table source: `from_table_id` has no open order to move — either
    /// genuinely free, or `occupied` with no order behind it (possible via
    /// `update_table_status` setting status by hand — see its doc comment).
    NoActiveOrder(String),
    /// Shift-table destination: `to_table_id` isn't `free` (name, its actual
    /// status) — merging two tables' orders into one is a different, bigger
    /// feature, so this is a hard reject, never a silent merge/overwrite.
    DestinationNotFree(String, String),
    /// Shift-table: `from_table_id == to_table_id` — nothing to move.
    SameTable,
}

impl std::fmt::Display for TableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TableError::TableNotFound => write!(f, "Table not found"),
            TableError::EmptyCart => write!(f, "Cart is empty"),
            TableError::EmptyName => write!(f, "Table name cannot be empty"),
            TableError::DuplicateName(name) => write!(f, "A table named \"{}\" already exists", name),
            TableError::InvalidSeats => write!(f, "Seats must be at least 1"),
            TableError::InvalidStatus(status) => write!(f, "Unknown table status: {}", status),
            TableError::Corrupt(msg) => write!(f, "Could not read the parked order: {}", msg),
            TableError::Sqlite(err) => write!(f, "database error: {}", err),
            TableError::NoActiveOrder(name) => write!(f, "{} doesn't have an active order to shift", name),
            TableError::DestinationNotFree(name, status) => {
                write!(f, "{} is already {} — choose a free table", name, status)
            }
            TableError::SameTable => write!(f, "Choose a different table to shift the order to"),
        }
    }
}

impl From<rusqlite::Error> for TableError {
    fn from(err: rusqlite::Error) -> Self {
        // SQLite reports a UNIQUE violation with this code; the caller (name
        // insert) already knows which name triggered it, so this fallback is
        // only reached from call sites that don't map it themselves.
        TableError::Sqlite(err)
    }
}

fn load_table(conn: &Connection, table_id: i64) -> Result<TableSummary, TableError> {
    conn.query_row(
        "SELECT t.id, t.name, t.seats, t.status,
                EXISTS (SELECT 1 FROM table_orders o
                         WHERE o.table_id = t.id AND o.status = 'open') AS has_parked
           FROM tables t
          WHERE t.id = ?1",
        params![table_id],
        |row| {
            Ok(TableSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                seats: row.get(2)?,
                status: row.get(3)?,
                has_parked_order: row.get::<_, i64>(4)? != 0,
            })
        },
    )
    .optional()?
    .ok_or(TableError::TableNotFound)
}

/// Every table with its status and whether one already has a cart parked on
/// it — the floor view, and the billing screen's table picker.
pub fn list_tables(conn: &Connection) -> Result<Vec<TableSummary>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.name, t.seats, t.status,
                EXISTS (SELECT 1 FROM table_orders o
                         WHERE o.table_id = t.id AND o.status = 'open') AS has_parked
           FROM tables t
          ORDER BY t.name",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(TableSummary {
            id: row.get(0)?,
            name: row.get(1)?,
            seats: row.get(2)?,
            status: row.get(3)?,
            has_parked_order: row.get::<_, i64>(4)? != 0,
        })
    })?;
    rows.collect()
}

/// Adds a new physical table to the floor. Owner/admin only at the command
/// layer — this function itself just validates the row.
pub fn add_table(conn: &Connection, name: &str, seats: i64) -> Result<TableSummary, TableError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(TableError::EmptyName);
    }
    if seats <= 0 {
        return Err(TableError::InvalidSeats);
    }

    let result = conn.execute(
        "INSERT INTO tables (name, seats, status) VALUES (?1, ?2, 'free')",
        params![name, seats],
    );
    match result {
        Ok(_) => load_table(conn, conn.last_insert_rowid()),
        Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == rusqlite::ErrorCode::ConstraintViolation => {
            Err(TableError::DuplicateName(name.to_string()))
        }
        Err(err) => Err(TableError::Sqlite(err)),
    }
}

/// Directly sets a table's status — for floor management that isn't driven
/// by an order (e.g. marking a table reserved for a booking, or freeing one
/// by hand). Does not touch any parked order; use `clear_table` to release a
/// table that has one.
pub fn update_table_status(conn: &Connection, table_id: i64, status: &str) -> Result<TableSummary, TableError> {
    if !TABLE_STATUSES.contains(&status) {
        return Err(TableError::InvalidStatus(status.to_string()));
    }
    let changed = conn.execute("UPDATE tables SET status = ?1 WHERE id = ?2", params![status, table_id])?;
    if changed == 0 {
        return Err(TableError::TableNotFound);
    }
    load_table(conn, table_id)
}

/// Parks (creates, or replaces the existing open one for) a table's draft
/// order and marks the table occupied. Used by "Save to table" on the
/// billing screen — the sale itself is not created until the table is billed.
pub fn attach_cart_to_table(
    conn: &Connection,
    table_id: i64,
    items: &[ParkedCartLine],
    discount_minor: i64,
) -> Result<(), TableError> {
    if items.is_empty() {
        return Err(TableError::EmptyCart);
    }

    let table_exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM tables WHERE id = ?1)",
        params![table_id],
        |row| row.get(0),
    )?;
    if !table_exists {
        return Err(TableError::TableNotFound);
    }

    let cart_json = serde_json::to_string(&ParkedCart { items: items.to_vec(), discount_minor })
        .map_err(|e| TableError::Corrupt(e.to_string()))?;

    upsert_open_order(conn, table_id, &cart_json)?;
    conn.execute("UPDATE tables SET status = 'occupied' WHERE id = ?1", params![table_id])?;
    load_table(conn, table_id)?;
    Ok(())
}

fn upsert_open_order(conn: &Connection, table_id: i64, cart_json: &str) -> Result<(), rusqlite::Error> {
    let existing_order_id: Option<i64> = conn
        .query_row(
            "SELECT id FROM table_orders WHERE table_id = ?1 AND status = 'open'",
            params![table_id],
            |row| row.get(0),
        )
        .optional()?;

    match existing_order_id {
        Some(order_id) => {
            conn.execute(
                "UPDATE table_orders SET cart_json = ?1 WHERE id = ?2",
                params![cart_json, order_id],
            )?;
        }
        None => {
            conn.execute(
                "INSERT INTO table_orders (table_id, status, cart_json) VALUES (?1, 'open', ?2)",
                params![table_id, cart_json],
            )?;
        }
    }
    Ok(())
}

/// "Seat a table": starts an empty draft order (if one isn't already open)
/// and marks the table occupied, so the billing screen has a table order to
/// attach a cart to as items are added. Idempotent — tapping an
/// already-occupied table with an open order just returns its current state.
pub fn start_table_order(conn: &Connection, table_id: i64) -> Result<TableSummary, TableError> {
    let table_exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM tables WHERE id = ?1)",
        params![table_id],
        |row| row.get(0),
    )?;
    if !table_exists {
        return Err(TableError::TableNotFound);
    }

    let has_open: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM table_orders WHERE table_id = ?1 AND status = 'open')",
        params![table_id],
        |row| row.get(0),
    )?;
    if !has_open {
        let empty_cart = serde_json::to_string(&ParkedCart { items: Vec::new(), discount_minor: 0 })
            .map_err(|e| TableError::Corrupt(e.to_string()))?;
        upsert_open_order(conn, table_id, &empty_cart)?;
    }

    conn.execute("UPDATE tables SET status = 'occupied' WHERE id = ?1", params![table_id])?;
    load_table(conn, table_id)
}

/// The cart currently parked on a table, if any — used to resume billing it.
pub fn get_parked_cart(conn: &Connection, table_id: i64) -> Result<Option<ParkedOrder>, TableError> {
    let cart_json: Option<String> = conn
        .query_row(
            "SELECT cart_json FROM table_orders WHERE table_id = ?1 AND status = 'open'",
            params![table_id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();

    let Some(cart_json) = cart_json else { return Ok(None) };
    let parsed: ParkedCart =
        serde_json::from_str(&cart_json).map_err(|e| TableError::Corrupt(e.to_string()))?;

    Ok(Some(ParkedOrder { items: parsed.items, discount_minor: parsed.discount_minor }))
}

/// Manually releases a table: cancels any open order for it and frees the
/// table. This is the "clear table" action from the floor view (a guest
/// leaving without paying, an order started by mistake, etc.) — a completed
/// sale takes the other path, `close_table_order_for_sale` below.
pub fn clear_table(conn: &Connection, table_id: i64) -> Result<TableSummary, TableError> {
    let table_exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM tables WHERE id = ?1)",
        params![table_id],
        |row| row.get(0),
    )?;
    if !table_exists {
        return Err(TableError::TableNotFound);
    }

    conn.execute(
        "UPDATE table_orders
            SET status = 'cancelled', closed_at = datetime('now', 'localtime')
          WHERE table_id = ?1 AND status = 'open'",
        params![table_id],
    )?;
    conn.execute("UPDATE tables SET status = 'free' WHERE id = ?1", params![table_id])?;
    load_table(conn, table_id)
}

/// Moves an in-progress order from one table to another — a customer who
/// gets up and sits somewhere else shouldn't force staff to start the bill
/// over. Only re-points the existing open `table_orders` row's `table_id`;
/// `cart_json` (items, quantities, discount) is never read or rewritten, so
/// nothing on the order is lost or reset by the move itself. Takes a
/// `&Transaction`, not a `&Connection`, like `close_table_order_for_sale`
/// below — this flips two tables' `status` and must not leave one freed
/// while the other fails to become occupied.
///
/// Rejects (never silently merges/overwrites):
/// - `from_table_id == to_table_id` (`SameTable`)
/// - `from_table_id` has no open order — `status != "occupied"` or no
///   `table_orders` row with `status = 'open'` (`NoActiveOrder`)
/// - `to_table_id` isn't `free` (`DestinationNotFree`) — a destination
///   already carrying its own order is a different, bigger feature
///   (merging two orders into one), out of scope here.
pub fn shift_table_order(
    tx: &Transaction,
    from_table_id: i64,
    to_table_id: i64,
) -> Result<ShiftTableResult, TableError> {
    if from_table_id == to_table_id {
        return Err(TableError::SameTable);
    }

    let from = load_table(tx, from_table_id)?;
    if from.status != "occupied" || !from.has_parked_order {
        return Err(TableError::NoActiveOrder(from.name));
    }

    let to = load_table(tx, to_table_id)?;
    if to.status != "free" {
        return Err(TableError::DestinationNotFree(to.name, to.status));
    }

    let changed = tx.execute(
        "UPDATE table_orders SET table_id = ?1 WHERE table_id = ?2 AND status = 'open'",
        params![to_table_id, from_table_id],
    )?;
    if changed == 0 {
        // Shouldn't happen given the has_parked_order check above (nothing
        // else can close an open order mid-transaction) — but never claim
        // success at moving nothing.
        return Err(TableError::NoActiveOrder(from.name));
    }

    tx.execute("UPDATE tables SET status = 'free' WHERE id = ?1", params![from_table_id])?;
    tx.execute("UPDATE tables SET status = 'occupied' WHERE id = ?1", params![to_table_id])?;

    Ok(ShiftTableResult { from: load_table(tx, from_table_id)?, to: load_table(tx, to_table_id)? })
}

/// Called from inside `sales::create_sale` when the completed sale has a
/// table: any still-open parked order for that table is closed out against
/// the new sale, and the table is freed for the next customers.
pub fn close_table_order_for_sale(tx: &Transaction, table_id: i64, sale_id: i64) -> Result<(), rusqlite::Error> {
    let existing_order_id: Option<i64> = tx
        .query_row(
            "SELECT id FROM table_orders WHERE table_id = ?1 AND status = 'open'",
            params![table_id],
            |row| row.get(0),
        )
        .optional()?;

    match existing_order_id {
        Some(order_id) => {
            tx.execute(
                "UPDATE table_orders
                    SET sale_id = ?1, status = 'billed', closed_at = datetime('now', 'localtime')
                  WHERE id = ?2",
                params![sale_id, order_id],
            )?;
        }
        None => {
            tx.execute(
                "INSERT INTO table_orders (table_id, sale_id, status, closed_at)
                 VALUES (?1, ?2, 'billed', datetime('now', 'localtime'))",
                params![table_id, sale_id],
            )?;
        }
    }

    tx.execute("UPDATE tables SET status = 'free' WHERE id = ?1", params![table_id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::sales::{self, CartLine, CreateSaleInput};
    use crate::db::schema::test_conn;

    fn item_id(conn: &Connection, name: &str) -> i64 {
        conn.query_row("SELECT id FROM items WHERE name = ?1", params![name], |row| row.get(0))
            .unwrap()
    }

    fn table_id(conn: &Connection, name: &str) -> i64 {
        conn.query_row("SELECT id FROM tables WHERE name = ?1", params![name], |row| row.get(0))
            .unwrap()
    }

    #[test]
    fn list_tables_reports_status_and_parked_flag() {
        let conn = test_conn();
        let cola = item_id(&conn, "Cola 500ml");
        let t3 = table_id(&conn, "Table 3");
        attach_cart_to_table(&conn, t3, &[ParkedCartLine { item_id: cola, qty: 1 }], 0).unwrap();

        let tables = list_tables(&conn).unwrap();
        assert_eq!(tables.len(), 6, "all six seeded tables should be listed");
        let table3 = tables.iter().find(|t| t.id == t3).unwrap();
        assert!(table3.has_parked_order);
        assert_eq!(table3.status, "occupied");
        assert_eq!(table3.seats, 4);
    }

    #[test]
    fn attaching_a_cart_twice_updates_the_same_open_order() {
        let conn = test_conn();
        let cola = item_id(&conn, "Cola 500ml");
        let t2 = table_id(&conn, "Table 2"); // seeded status: free

        attach_cart_to_table(&conn, t2, &[ParkedCartLine { item_id: cola, qty: 1 }], 0).unwrap();
        attach_cart_to_table(&conn, t2, &[ParkedCartLine { item_id: cola, qty: 4 }], 200).unwrap();

        let order_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM table_orders WHERE table_id = ?1",
                params![t2],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(order_count, 1, "second attach should update, not duplicate");

        let parked = get_parked_cart(&conn, t2).unwrap().unwrap();
        assert_eq!(parked.items[0].qty, 4);
        assert_eq!(parked.discount_minor, 200);

        let status: String =
            conn.query_row("SELECT status FROM tables WHERE id = ?1", params![t2], |row| row.get(0)).unwrap();
        assert_eq!(status, "occupied");
    }

    #[test]
    fn attach_cart_rejects_empty_items_and_unknown_table() {
        let conn = test_conn();
        let cola = item_id(&conn, "Cola 500ml");
        let t2 = table_id(&conn, "Table 2");

        assert!(matches!(attach_cart_to_table(&conn, t2, &[], 0), Err(TableError::EmptyCart)));
        assert!(matches!(
            attach_cart_to_table(&conn, 999_999, &[ParkedCartLine { item_id: cola, qty: 1 }], 0),
            Err(TableError::TableNotFound)
        ));
    }

    #[test]
    fn add_table_creates_a_free_table_with_the_given_seats() {
        let conn = test_conn();
        let table = add_table(&conn, "Patio 1", 6).unwrap();
        assert_eq!(table.name, "Patio 1");
        assert_eq!(table.seats, 6);
        assert_eq!(table.status, "free");
        assert!(!table.has_parked_order);
    }

    #[test]
    fn add_table_rejects_a_duplicate_name() {
        let conn = test_conn();
        let err = add_table(&conn, "Table 1", 4).unwrap_err();
        assert!(matches!(err, TableError::DuplicateName(_)));
    }

    #[test]
    fn add_table_rejects_a_blank_name_or_non_positive_seats() {
        let conn = test_conn();
        assert!(matches!(add_table(&conn, "   ", 4), Err(TableError::EmptyName)));
        assert!(matches!(add_table(&conn, "Patio 2", 0), Err(TableError::InvalidSeats)));
    }

    #[test]
    fn update_table_status_changes_the_row_and_rejects_unknown_statuses() {
        let conn = test_conn();
        let t2 = table_id(&conn, "Table 2"); // seeded free
        let updated = update_table_status(&conn, t2, "reserved").unwrap();
        assert_eq!(updated.status, "reserved");

        assert!(matches!(
            update_table_status(&conn, t2, "on-fire"),
            Err(TableError::InvalidStatus(_))
        ));
        assert!(matches!(
            update_table_status(&conn, 999_999, "free"),
            Err(TableError::TableNotFound)
        ));
    }

    #[test]
    fn start_table_order_seats_a_free_table_with_an_empty_draft() {
        let conn = test_conn();
        let t2 = table_id(&conn, "Table 2"); // seeded free
        let table = start_table_order(&conn, t2).unwrap();
        assert_eq!(table.status, "occupied");
        assert!(table.has_parked_order);

        let parked = get_parked_cart(&conn, t2).unwrap().unwrap();
        assert!(parked.items.is_empty());
    }

    #[test]
    fn start_table_order_is_idempotent_and_does_not_wipe_an_existing_cart() {
        let conn = test_conn();
        let cola = item_id(&conn, "Cola 500ml");
        let t2 = table_id(&conn, "Table 2");
        attach_cart_to_table(&conn, t2, &[ParkedCartLine { item_id: cola, qty: 3 }], 0).unwrap();

        start_table_order(&conn, t2).unwrap();

        let parked = get_parked_cart(&conn, t2).unwrap().unwrap();
        assert_eq!(parked.items[0].qty, 3, "re-seating an already-open table must not discard its cart");

        let order_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM table_orders WHERE table_id = ?1", params![t2], |row| row.get(0))
            .unwrap();
        assert_eq!(order_count, 1);
    }

    #[test]
    fn clear_table_cancels_the_open_order_and_frees_the_table() {
        let conn = test_conn();
        let cola = item_id(&conn, "Cola 500ml");
        let t1 = table_id(&conn, "Table 1"); // seeded occupied
        attach_cart_to_table(&conn, t1, &[ParkedCartLine { item_id: cola, qty: 2 }], 0).unwrap();

        let table = clear_table(&conn, t1).unwrap();
        assert_eq!(table.status, "free");
        assert!(!table.has_parked_order);

        let order_status: String = conn
            .query_row(
                "SELECT status FROM table_orders WHERE table_id = ?1 ORDER BY id DESC LIMIT 1",
                params![t1],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(order_status, "cancelled");
    }

    #[test]
    fn clear_table_on_a_table_with_no_order_just_frees_it() {
        let conn = test_conn();
        let t4 = table_id(&conn, "Table 4"); // seeded reserved, no order
        let table = clear_table(&conn, t4).unwrap();
        assert_eq!(table.status, "free");
    }

    /// The full restaurant loop end to end: a free table is seated (starts an
    /// order, goes occupied), items are added to it as the cashier bills the
    /// table (parking the cart, same as the billing screen's "Save to
    /// table"), the sale completes, and the table comes back to `free` with
    /// `sales.table_id` correctly pointing at it.
    #[test]
    fn full_loop_seat_order_bill_and_the_table_returns_to_free() {
        let mut conn = test_conn();
        let cola = item_id(&conn, "Cola 500ml");
        let t2 = table_id(&conn, "Table 2"); // seeded free

        // 1. Mark the table occupied by starting an order.
        let seated = start_table_order(&conn, t2).unwrap();
        assert_eq!(seated.status, "occupied");

        // 2. Add items via billing (parks the cart on the table's open order).
        attach_cart_to_table(&conn, t2, &[ParkedCartLine { item_id: cola, qty: 2 }], 0).unwrap();
        assert!(get_parked_cart(&conn, t2).unwrap().unwrap().items.iter().any(|l| l.item_id == cola));

        // 3. Complete the sale against that table.
        let tx = conn.transaction().unwrap();
        let sale = sales::create_sale(
            &tx,
            CreateSaleInput {
                items: vec![CartLine { item_id: cola, qty: 2, notes: None }],
                discount_minor: 0,
                tax_minor: 0,
                payment_method: "cash".into(),
                cashier_id: None,
                table_id: Some(t2),
                shift_id: None,
            },
        )
        .unwrap();
        tx.commit().unwrap();

        // 4. The table is free again, and the sale is linked to it.
        let after = load_table(&conn, t2).unwrap();
        assert_eq!(after.status, "free", "table must auto-clear once the sale completes");
        assert!(!after.has_parked_order);

        let linked_table_id: Option<i64> = conn
            .query_row("SELECT table_id FROM sales WHERE id = ?1", params![sale.id], |row| row.get(0))
            .unwrap();
        assert_eq!(linked_table_id, Some(t2), "sales.table_id must point at the table that was billed");
    }

    #[test]
    fn shift_table_order_moves_the_order_and_flips_both_statuses() {
        let mut conn = test_conn();
        let cola = item_id(&conn, "Cola 500ml");
        let t1 = table_id(&conn, "Table 1"); // seeded occupied
        let t2 = table_id(&conn, "Table 2"); // seeded free
        attach_cart_to_table(&conn, t1, &[ParkedCartLine { item_id: cola, qty: 3 }], 500).unwrap();

        let tx = conn.transaction().unwrap();
        let result = shift_table_order(&tx, t1, t2).unwrap();
        assert_eq!(result.from.status, "free");
        assert!(!result.from.has_parked_order);
        assert_eq!(result.to.status, "occupied");
        assert!(result.to.has_parked_order);
        tx.commit().unwrap();

        // The cart itself — items, qty, discount — must carry over untouched:
        // same open order, just re-pointed, not rebuilt.
        let parked = get_parked_cart(&conn, t2).unwrap().unwrap();
        assert_eq!(parked.items.len(), 1);
        assert_eq!(parked.items[0].item_id, cola);
        assert_eq!(parked.items[0].qty, 3);
        assert_eq!(parked.discount_minor, 500);
        assert!(get_parked_cart(&conn, t1).unwrap().is_none(), "old table must have no order left");

        let order_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM table_orders WHERE table_id = ?1", params![t2], |row| row.get(0))
            .unwrap();
        assert_eq!(order_count, 1, "must move the existing order, not create a second one");
    }

    #[test]
    fn shift_table_order_rejects_a_source_with_no_active_order() {
        let mut conn = test_conn();
        let t2 = table_id(&conn, "Table 2"); // seeded free, no order
        let t3 = table_id(&conn, "Table 3"); // seeded free

        let tx = conn.transaction().unwrap();
        assert!(matches!(shift_table_order(&tx, t2, t3), Err(TableError::NoActiveOrder(_))));
    }

    #[test]
    fn shift_table_order_rejects_an_occupied_status_with_no_backing_order() {
        // `update_table_status` can mark a table "occupied" by hand with no
        // `table_orders` row behind it — `shift_table_order` must not trust
        // `status` alone.
        let mut conn = test_conn();
        let t2 = table_id(&conn, "Table 2");
        update_table_status(&conn, t2, "occupied").unwrap();
        let t3 = table_id(&conn, "Table 3");

        let tx = conn.transaction().unwrap();
        assert!(matches!(shift_table_order(&tx, t2, t3), Err(TableError::NoActiveOrder(_))));
    }

    #[test]
    fn shift_table_order_rejects_a_destination_that_is_not_free() {
        let mut conn = test_conn();
        let cola = item_id(&conn, "Cola 500ml");
        let t1 = table_id(&conn, "Table 1"); // seeded occupied
        attach_cart_to_table(&conn, t1, &[ParkedCartLine { item_id: cola, qty: 1 }], 0).unwrap();
        let t4 = table_id(&conn, "Table 4"); // seeded reserved

        let tx = conn.transaction().unwrap();
        let err = shift_table_order(&tx, t1, t4).unwrap_err();
        assert!(matches!(err, TableError::DestinationNotFree(_, _)));
        assert!(err.to_string().contains("already reserved"));

        // Nothing must have moved or changed status on a rejected shift.
        let still_at_t1 = get_parked_cart(&tx, t1).unwrap();
        assert!(still_at_t1.is_some(), "order must stay on the source table when the shift is rejected");
    }

    #[test]
    fn shift_table_order_rejects_shifting_a_table_to_itself() {
        let mut conn = test_conn();
        let cola = item_id(&conn, "Cola 500ml");
        let t1 = table_id(&conn, "Table 1");
        attach_cart_to_table(&conn, t1, &[ParkedCartLine { item_id: cola, qty: 1 }], 0).unwrap();

        let tx = conn.transaction().unwrap();
        assert!(matches!(shift_table_order(&tx, t1, t1), Err(TableError::SameTable)));
    }

    #[test]
    fn a_completed_sale_closes_the_parked_order_and_frees_the_table() {
        let mut conn = test_conn();
        let cola = item_id(&conn, "Cola 500ml");
        let t1 = table_id(&conn, "Table 1"); // seeded status: occupied

        attach_cart_to_table(&conn, t1, &[ParkedCartLine { item_id: cola, qty: 2 }], 0).unwrap();

        let tx = conn.transaction().unwrap();
        let input = CreateSaleInput {
            items: vec![CartLine { item_id: cola, qty: 2, notes: None }],
            discount_minor: 0,
            tax_minor: 0,
            payment_method: "cash".into(),
            cashier_id: None,
            table_id: Some(t1),
            shift_id: None,
        };
        let sale = sales::create_sale(&tx, input).unwrap();
        tx.commit().unwrap();

        let (order_status, order_sale_id): (String, Option<i64>) = conn
            .query_row(
                "SELECT status, sale_id FROM table_orders WHERE table_id = ?1",
                params![t1],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(order_status, "billed");
        assert_eq!(order_sale_id, Some(sale.id));

        let status_after: String =
            conn.query_row("SELECT status FROM tables WHERE id = ?1", params![t1], |row| row.get(0)).unwrap();
        assert_eq!(status_after, "free");

        assert!(get_parked_cart(&conn, t1).unwrap().is_none(), "parked cart should be gone once billed");
    }
}
